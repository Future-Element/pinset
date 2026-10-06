"""Real Secret Service, Java wrappers/editor host, and Flutter metadata contracts."""
import json, os, shutil, subprocess, tempfile, time, urllib.request, zipfile
import xml.etree.ElementTree as ET
from pathlib import Path
from support import *
from acceptance import select_tool
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from verification_policy import RUNTIME_EXEMPTIONS
WRAPPER_OBSERVATIONS=[]
DRIFT_OBSERVATIONS=[]

def credentials():
    root=project('secret-service')
    data(root,'env','access','request')
    data(root,'env','init','development')
    value='ephemeral-'+os.urandom(16).hex()
    cli(root,'env','set','APP_INTEGRATION_TOKEN','--profile','development','--stdin',stdin=value)
    encrypted=(root/'.pinset/env/development.env').read_text()
    assert value not in encrypted and 'encrypted:pinset:v3:' in encrypted
    assert not list((root/'.pinset/env').glob('*.lock'))
    data(root,'env','use','development')
    cli(root,'exec','--','sh','-c','test -n "$APP_INTEGRATION_TOKEN" && test -z "${PINSET_IDENTITY:-}"',native=True)
    cli(root,'exec','--no-env','--','sh','-c','test -z "${APP_INTEGRATION_TOKEN:-}"',native=True)
    original_home=os.environ['PINSET_HOME'];secondary=tempfile.mkdtemp(prefix='pinset-second-device-')
    os.environ['PINSET_HOME']=secondary
    request=data(root,'env','access','request','--new')
    os.environ['PINSET_HOME']=original_home
    data(root,'env','access','grant',request['file'],'--profile','development')
    os.environ['PINSET_HOME']=secondary
    data(root,'env','trust','add')
    cli(root,'exec','--profile','development','--','sh','-c','test -n "$APP_INTEGRATION_TOKEN"',native=True)
    os.environ['PINSET_HOME']=original_home
    data(root,'env','access','revoke',request['request']['id'],'--profile','development')
    os.environ['PINSET_HOME']=secondary;data(root,'env','trust','add')
    cli(root,'exec','--profile','development','--','sh','-c','exit 0',native=True,expected=1)
    os.environ['PINSET_HOME']=original_home
    (root/'.pinset/config.toml').write_text((root/'.pinset/config.toml').read_text()+'\n# external change\n')
    # Canonical config content is fingerprinted; change public policy, not comments.
    config=root/'.pinset/config.toml';before=config.read_text()
    changed=before.replace('[policy]', '[policy]\nminimum_release_age = "1d"')
    assert changed!=before, 'trust invalidation fixture did not change configuration'
    config.write_text(changed)
    assert data(root,'env','trust','status')['trusted'] is False
    cli(root,'env','set','OTHER','--profile','development','--stdin',stdin='blocked',expected=1)
    data(root,'env','trust','add')
    cli(root,'env','set','JAVA_HOME','--profile','development','--stdin',stdin='blocked',expected=1)
    assert 'AGE-SECRET-KEY-' not in '\n'.join(p.read_text(errors='ignore') for p in Path(original_home).rglob('*') if p.is_file() and p.stat().st_size<1024*1024)
    return root

def wrappers():
    projects=[]
    for feature,gradle in [('8','6.9.4'),('11','6.9.4'),('17','7.6.4'),('21','8.14.3'),('25','9.1.0'),('latest','9.8.0')]:
        root=project('wrappers-'+feature);select_tool(root,'java@'+feature)
        sdk=data(root,'which','java')['sdk'];projects.append(root)
        # Maven's own wrapper prepares its pinned distribution. No Maven Provider is involved.
        wrapper=download('https://repo.maven.apache.org/maven2/org/apache/maven/wrapper/maven-wrapper-distribution/3.3.2/maven-wrapper-distribution-3.3.2-only-script.zip',
          '81f6e5505d44263ef4ebf47935dd552c88de7466a6443d8738406fa8f88e0a03','maven-wrapper-3.3.2.zip')
        with zipfile.ZipFile(wrapper) as archive:archive.extract('mvnw',root)
        (root/'mvnw').chmod(0o755)
        (root/'.mvn/wrapper').mkdir(parents=True)
        (root/'.mvn/wrapper/maven-wrapper.properties').write_text('wrapperVersion=3.3.2\ndistributionType=only-script\ndistributionUrl=https://repo.maven.apache.org/maven2/org/apache/maven/apache-maven/3.9.11/apache-maven-3.9.11-bin.zip\n')
        (root/'pom.xml').write_text('<project><modelVersion>4.0.0</modelVersion><groupId>dev.pinset</groupId><artifactId>jdk-test</artifactId><version>1</version><properties><maven.compiler.source>8</maven.compiler.source><maven.compiler.target>8</maven.compiler.target></properties><dependencies><dependency><groupId>junit</groupId><artifactId>junit</artifactId><version>4.13.2</version><scope>test</scope></dependency></dependencies><build><plugins><plugin><groupId>org.apache.maven.plugins</groupId><artifactId>maven-surefire-plugin</artifactId><version>3.5.4</version></plugin></plugins></build></project>')
        (root/'src/main/java').mkdir(parents=True);(root/'src/main/java/Main.java').write_text('public class Main { public static String value() { return "WRAPPER_OK"; } public static void main(String[] a) { System.out.println(value()); }}')
        (root/'src/test/java').mkdir(parents=True);(root/'src/test/java/MainTest.java').write_text('import org.junit.Test; import static org.junit.Assert.*; public class MainTest { @Test public void compiledProjectWorks() { assertEquals("WRAPPER_OK",Main.value()); System.out.println("TEST_RUNTIME_JDK="+System.getProperty("java.home")); }}')
        version=native(root,'./mvnw','-version',timeout=600);assert sdk in version,version
        native(root,'./mvnw','verify',timeout=1200);assert (root/'target/jdk-test-1.jar').exists()
        maven_tests=ET.parse(root/'target/surefire-reports/TEST-MainTest.xml').getroot()
        assert int(maven_tests.attrib['tests'])>0 and int(maven_tests.attrib['failures'])==int(maven_tests.attrib['errors'])==0
        assert any(p.attrib.get('name')=='java.home' and p.attrib.get('value') in [sdk,sdk+'/jre'] for p in maven_tests.find('properties'))
        distribution=official_download(f'https://services.gradle.org/distributions/gradle-{gradle}-bin.zip',f'gradle-{gradle}.zip')
        directory=CACHE/f'gradle-{gradle}'
        if not directory.exists():
            with zipfile.ZipFile(distribution) as archive:archive.extractall(CACHE)
            (directory/'bin/gradle').chmod(0o755)
        (root/'settings.gradle').write_text('rootProject.name="pinset-jdk-test"')
        (root/'build.gradle').write_text('plugins { id "java" }\nrepositories { mavenCentral() }\ndependencies { testImplementation "junit:junit:4.13.2" }\ntest { testLogging.showStandardStreams = true }\njava { sourceCompatibility = JavaVersion.VERSION_1_8; targetCompatibility = JavaVersion.VERSION_1_8 }\ntasks.withType(JavaCompile).configureEach { task -> doFirst { println("COMPILER_CONFIG_JDK=" + (task.hasProperty("javaCompiler") && task.javaCompiler.present ? task.javaCompiler.get().metadata.installationPath.asFile : System.getProperty("java.home"))); }}\ntasks.register("jdkEvidence") { doLast { println("RUNTIME_JDK="+System.getProperty("java.home"));  }}')
        # Reuse the already checksum-verified official distribution in the test wrapper.
        native(root,str(directory/'bin/gradle'),'wrapper','--gradle-distribution-url',distribution.as_uri(),'--no-daemon',timeout=600)
        properties=root/'gradle/wrapper/gradle-wrapper.properties'
        properties.write_text(properties.read_text()+'\ndistributionSha256Sum='+__import__('hashlib').sha256(distribution.read_bytes()).hexdigest()+'\n')
        compiled=native(root,'./gradlew','build','jdkEvidence','--no-daemon',timeout=1200)
        assert 'COMPILER_CONFIG_JDK='+sdk in compiled or 'COMPILER_CONFIG_JDK='+sdk+'/jre' in compiled,compiled
        assert list((root/'build/libs').glob('*.jar'))
        gradle_tests=ET.parse(root/'build/test-results/test/TEST-MainTest.xml').getroot()
        assert int(gradle_tests.attrib['tests'])>0 and int(gradle_tests.attrib['failures'])==int(gradle_tests.attrib['errors'])==0
        assert 'TEST_RUNTIME_JDK='+sdk in compiled or 'TEST_RUNTIME_JDK='+sdk+'/jre' in compiled,compiled
        observed=data(root,'check','--probe',timeout=300)
        assert observed['java']['build_runtime_jdk']['observations']['gradle']['runtime_matches_lock'],observed
        assert observed['java']['build_runtime_jdk']['observations']['maven']['runtime_matches_lock'],observed
        WRAPPER_OBSERVATIONS.append(observed['java'])
    # A successful build can deliberately use a compiler or build JVM outside the project JDK.
    root=projects[3];external=data(projects[2],'which','java')['sdk']
    build=root/'build.gradle';build.write_text(build.read_text()+'\njava { toolchain { languageVersion = JavaLanguageVersion.of(17) } }\n')
    properties=root/'gradle.properties';properties.write_text('org.gradle.java.installations.auto-download=false\norg.gradle.java.installations.paths='+external+'\n')
    output=native(root,'./gradlew','clean','build','jdkEvidence','--no-daemon',timeout=1200)
    assert 'COMPILER_CONFIG_JDK='+external in output,output
    observed=data(root,'check','--probe',timeout=300)
    compilers=observed['java']['compiler_toolchain_jdk']['configuration_observations']
    assert any(item['external'] and item['home']==external for item in compilers),observed
    assert not any(item['compilation_observed'] for item in compilers),observed
    DRIFT_OBSERVATIONS.append(observed['java'])
    properties.write_text(properties.read_text()+'org.gradle.java.home='+external+'\n')
    output=native(root,'./gradlew','jdkEvidence',timeout=600)
    assert 'RUNTIME_JDK='+external in output,output
    observed=data(root,'check','--probe',timeout=300)
    assert not observed['java']['build_runtime_jdk']['observations']['gradle']['runtime_matches_lock'],observed
    DRIFT_OBSERVATIONS.append(observed['java'])
    # Restore the test declaration; the real editor host remains a normal locked-JDK project.
    properties.write_text(properties.read_text().replace('org.gradle.java.home='+external+'\n',''))
    native(root,'./gradlew','--stop',timeout=300)
    return projects

def flutter_contracts():
    # Deliberately resolve metadata only. No Flutter/Android SDK archive is downloaded.
    root=project('flutter-contracts')
    data(root,'use',frozen_selector('flutter@3.35.4'),'--no-install',timeout=1200)
    lock=__import__('tomllib').loads((root/'.pinset/lock.toml').read_text())['tool'][0]
    assert lock['name']=='flutter' and lock['metadata']['dart_version']
    assert all(artifact['sha256'] and artifact['canonical_url'].startswith('https://storage.googleapis.com/flutter_infra_release/') for artifact in lock['artifact'])
    freeze_selector('flutter@3.35.4',lock['version'])
    before=(root/'.pinset/lock.toml').read_bytes()
    data(root,'install','--plan')
    assert (root/'.pinset/lock.toml').read_bytes()==before
    for command in ['flutter','dart']:
        assert data(root,'which',command,expected=1)['error']['code']=='PINSET_INSTALL_MISSING'
    checked=data(root,'check','--target','android')
    assert not checked['android']['verified'] and not checked['android']['flutter']['actual_jdk_observed']
    assert not any(item['actually_verified'] for item in checked['report']['checks'])
    report('flutter-contracts',status='passed',scope='Official metadata and read-only command/lock contracts; no SDK execution',
           lock=lock,android_read_only=checked['android'],runtime_exemptions=RUNTIME_EXEMPTIONS)
    return checked

def gui(root):
    executable=Path(data(root,'which','jconsole')['executable'])
    log=REPORTS/'jconsole-startup.log'
    with log.open('w') as output:
        process=subprocess.Popen([BIN,'-C',root,'exec','--no-env','--','jconsole'],stdout=output,stderr=subprocess.STDOUT,start_new_session=True)
        try:time.sleep(5);assert process.poll() is None,'jconsole failed to stay running under Xvfb'
        finally:
            import signal
            os.killpg(process.pid,signal.SIGTERM);process.wait(timeout=30)
    return {'tool':'jconsole','inventory':'present','startup':'observed under Xvfb','interactions':'not exercised'}

if __name__=='__main__':
    import sys
    if sys.argv[1:]==['credentials']:
        credentials();report('credentials',status='passed',secret_service='real D-Bus Secret Service');raise SystemExit(0)
    if sys.argv[1:]==['wrappers']:
        projects=wrappers();report('wrappers',status='passed',projects=list(map(str,projects)),observations=WRAPPER_OBSERVATIONS,drift=DRIFT_OBSERVATIONS);raise SystemExit(0)
    if sys.argv[1:]==['flutter']:
        flutter_contracts();raise SystemExit(0)
    secret=credentials();java=wrappers();gui_result=gui(java[-1]);flutter_contracts()
    run(['node','scripts/tests/editor-host.cjs',str(java[-1])],timeout=1200)
    report('integrations',status='passed',secret_service='real D-Bus Secret Service',java_gui=gui_result,
           runtime_exemptions=RUNTIME_EXEMPTIONS,java_wrappers=WRAPPER_OBSERVATIONS,external_jdk_drift=DRIFT_OBSERVATIONS)
