"""Authorization provenance and fail-closed mutations with memory-only test identities."""
import json, os, re, subprocess, tempfile, uuid
from pathlib import Path
from support import cli, data


def _bech32(hrp, raw):
    alphabet = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l'
    encoded = []
    accumulator = bits = 0
    for byte in raw:
        accumulator = (accumulator << 8) | byte
        bits += 8
        while bits >= 5:
            bits -= 5
            encoded.append((accumulator >> bits) & 31)
    if bits:
        encoded.append((accumulator << (5 - bits)) & 31)
    values = ([ord(char) >> 5 for char in hrp] + [0]
              + [ord(char) & 31 for char in hrp] + encoded + [0] * 6)
    checksum = 1
    generators = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3]
    for value in values:
        top = checksum >> 25
        checksum = ((checksum & 0x1ffffff) << 5) ^ value
        for index, generator in enumerate(generators):
            if (top >> index) & 1:
                checksum ^= generator
    checksum ^= 1
    return hrp + '1' + ''.join(alphabet[value] for value in encoded + [
        (checksum >> (5 * (5 - index))) & 31 for index in range(6)
    ])


def _identity():
    # DER stays in memory. Neither private material nor decrypted values are logged.
    private = subprocess.check_output(['openssl', 'genpkey', '-algorithm', 'X25519', '-outform', 'DER'])
    public = subprocess.check_output(['openssl', 'pkey', '-inform', 'DER', '-pubout', '-outform', 'DER'], input=private)
    assert len(private) == 48 and len(public) == 44
    return _bech32('age-secret-key-', private[-32:]).upper(), _bech32('age', public[-32:])


def verify_access_regressions():
    with tempfile.TemporaryDirectory(prefix='pinset-access-regression-') as temporary:
        scratch = Path(temporary)
        project = scratch / 'project'
        project.mkdir()
        owner, owner_public = _identity()
        first, first_public = _identity()
        second, second_public = _identity()
        environment = os.environ.copy()
        environment.update(PINSET_HOME=str(scratch / 'home'), HOME=str(scratch), PINSET_IDENTITY=owner)
        for key in ['PINSET_PROFILE', 'PINSET_NO_ENV', 'PINSET_ENV_RESOLVED']:
            environment.pop(key, None)
        environment.update(HTTP_PROXY='http://127.0.0.1:1', HTTPS_PROXY='http://127.0.0.1:1',
                           ALL_PROXY='http://127.0.0.1:1', NO_PROXY='')

        def request(recipient, request_id=None):
            request_id = request_id or str(uuid.uuid4())
            path = scratch / (request_id + '.json')
            path.write_text(json.dumps({'protocol': 'pinset/3', 'id': request_id,
                                        'recipient': recipient, 'created_at': 0}))
            return path, request_id

        def state():
            return {str(path.relative_to(scratch)): path.read_bytes()
                    for base in [project / '.pinset', scratch / 'home']
                    for path in base.rglob('*') if path.is_file()}

        def denied(code, *arguments, **kwargs):
            before = state()
            result = data(project, *arguments, expected=1, env=environment, **kwargs)
            assert result['error']['code'] == code, result
            assert state() == before, 'rejected authorization edit changed profile or state'
            assert not (project / '.pinset/local/transaction.json').exists()
            return result

        def can_decrypt(identity, expected=0):
            child_environment = environment.copy()
            child_environment['PINSET_IDENTITY'] = identity
            cli(project, 'exec', '--profile', 'development', '--', '/bin/sh', '-c',
                'test -n "$ACCESS_REGRESSION_TOKEN" && test -z "${PINSET_IDENTITY:-}"',
                native=True, expected=expected, env=child_environment)

        data(project, 'init', env=environment)
        data(project, 'env', 'init', 'development', env=environment)
        cli(project, 'env', 'set', 'ACCESS_REGRESSION_TOKEN', '--profile', 'development',
            '--stdin', stdin='before-revocation', env=environment)
        profile_arguments = ['--profile', 'development']
        path, request_id = request(first_public)
        result = data(project, 'env', 'access', 'grant', path, *profile_arguments, env=environment)
        assert result['changed']
        can_decrypt(first)
        before = state()
        result = data(project, 'env', 'access', 'grant', path, *profile_arguments, env=environment)
        assert not result['changed'] and state() == before, 'idempotent grant rewrote ciphertext or state'
        request(second_public, request_id)
        denied('PINSET_ACCESS_REQUEST_CONFLICT', 'env', 'access', 'grant', path, *profile_arguments)
        can_decrypt(second, expected=1)
        data(project, 'env', 'access', 'revoke', request_id, *profile_arguments, env=environment)
        cli(project, 'env', 'set', 'ACCESS_REGRESSION_TOKEN', *profile_arguments,
            '--stdin', stdin='fresh-after-revocation', env=environment)
        can_decrypt(first, expected=1)
        assert data(project, 'env', 'access', 'list', *profile_arguments, env=environment)['grants'] == {}

        # Removing a grant must retain any independent owner or grant source.
        owner_path, owner_id = request(owner_public)
        data(project, 'env', 'access', 'grant', owner_path, *profile_arguments, env=environment)
        data(project, 'env', 'access', 'revoke', owner_id, *profile_arguments, env=environment)
        can_decrypt(owner)
        first_path, first_id = request(first_public)
        alias_path, alias_id = request(first_public)
        for path in [first_path, alias_path]:
            data(project, 'env', 'access', 'grant', path, *profile_arguments, env=environment)
        data(project, 'env', 'access', 'revoke', first_id, *profile_arguments, env=environment)
        can_decrypt(first)
        data(project, 'env', 'access', 'revoke', alias_id, *profile_arguments, env=environment)
        can_decrypt(first, expected=1)
        can_decrypt(owner)
        denied('PINSET_ACCESS_MISSING', 'env', 'access', 'revoke', alias_id, *profile_arguments)

        config = project / '.pinset/config.toml'
        current = config.read_text()
        # Old metadata establishes encryption recipients, never owner provenance.
        legacy = re.sub(r'(?m)^owner_recipients = .*\n', '', current)
        assert legacy != current
        config.write_text(legacy)
        data(project, 'env', 'trust', 'add', env=environment)
        can_decrypt(owner)
        for arguments, kwargs in [
            (['env', 'set', 'ACCESS_REGRESSION_TOKEN', *profile_arguments, '--stdin'], {'stdin': 'blocked'}),
            (['env', 'unset', 'ACCESS_REGRESSION_TOKEN', *profile_arguments], {}),
            (['env', 'access', 'grant', first_path, *profile_arguments], {}),
            (['env', 'access', 'revoke', first_id, *profile_arguments], {}),
        ]:
            result = denied('PINSET_ACCESS_ROLES_REQUIRED', *arguments, **kwargs)
            assert 'owner_recipients' in result['error']['message'] and 'pinset env trust add' in result['error']['message']

        # Explicit ownership does not authorize silent repair of orphaned recipients.
        mismatch = re.sub(r'(?m)^recipients = .*$',
                          'recipients = ' + json.dumps([owner_public, second_public]), current, count=1)
        assert mismatch != current
        config.write_text(mismatch)
        denied('PINSET_ACCESS_METADATA_INVALID', 'env', 'trust', 'add')
        denied('PINSET_ACCESS_METADATA_INVALID', 'env', 'set', 'ACCESS_REGRESSION_TOKEN',
               *profile_arguments, '--stdin', stdin='blocked')
        denied('PINSET_ACCESS_METADATA_INVALID', 'env', 'access', 'grant', first_path, *profile_arguments)
        empty = re.sub(r'(?m)^owner_recipients = .*$', 'owner_recipients = []', current)
        assert empty != current
        config.write_text(empty)
        denied('PINSET_ACCESS_OWNER_REQUIRED', 'env', 'trust', 'add')
        denied('PINSET_ACCESS_OWNER_REQUIRED', 'env', 'unset', 'ACCESS_REGRESSION_TOKEN', *profile_arguments)
        config.write_text(current)
        data(project, 'env', 'trust', 'add', env=environment)
        can_decrypt(owner)

        denied('PINSET_ENVIRONMENT', 'env', 'set', 'ACCESS_REGRESSION_TOKEN',
               *profile_arguments, '--stdin', stdin='invalid\x00value')
        denied('PINSET_ENVIRONMENT', 'env', 'set', 'ACCESS_REGRESSION_TOKEN',
               *profile_arguments, '--stdin', stdin='x' * 900000)
        profile_path = project / '.pinset/env/development.env'
        saved_ciphertext = profile_path.read_bytes()
        try:
            profile_path.write_text('invalid encrypted profile\n')
            data(project, 'env', 'trust', 'add', env=environment)
            denied('PINSET_ENVIRONMENT', 'env', 'set', 'ACCESS_REGRESSION_TOKEN',
                   *profile_arguments, '--stdin', stdin='blocked')
            denied('PINSET_ENVIRONMENT', 'env', 'unset', 'ACCESS_REGRESSION_TOKEN', *profile_arguments)
        finally:
            profile_path.write_bytes(saved_ciphertext)
            data(project, 'env', 'trust', 'add', env=environment)
        can_decrypt(owner)

        assert not (project / '.pinset/local/transaction.json').exists()
        assert 'AGE-SECRET-KEY-' not in '\n'.join(
            path.read_text(errors='ignore') for path in scratch.rglob('*')
            if path.is_file() and path.stat().st_size < 1024 * 1024
        )
    return {'request_id_conflicts': 'rejected without writes', 'same_request': 'idempotent without writes',
            'revocation': 'fresh ciphertext denies removed recipient', 'owner_sources': 'retained',
            'shared_grant_sources': 'retained until last grant is removed',
            'ambiguous_legacy_roles': 'read-only decryption allowed; every mutation fails before transaction',
            'inconsistent_metadata': 'rejected without automatic permission repair',
            'invalid_input_or_ciphertext': 'prepared before transaction; rejection preserves project state'}


if __name__ == '__main__':
    print(json.dumps(verify_access_regressions(), sort_keys=True))
