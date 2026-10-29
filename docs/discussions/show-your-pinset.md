# Show your .pinset

<!-- GitHub Discussion body draft. Not published. -->

What tools does your project need, and how do you keep their versions together?

Share your `.pinset/config.toml` and the relevant parts of `.pinset/lock.toml`, along with:

- Your project type and operating system/architecture.
- The toolchains you selected and why.
- A command you run with `pinset exec`, or a `pinset which <command> --explain` result that helped you understand routing.
- Anything that was confusing or did not work as expected.

For example, a small Node.js, Python and Go project can start with:

```toml
protocol = "pinset/3"
schema = 3
project_id = "polyglot-example"

[tools]
node = "24"
python = "3.14"
go = "1.25"
```

Run `pinset use node@24 python@3.14 go@1.25 --no-install` to generate the exact lock, then `pinset install` to install it. Commit the configuration and lock together so another checkout can install the same selected artifacts. A runnable template is in [`examples/polyglot`](https://github.com/Future-Element/pinset/tree/main/examples/polyglot).

Installation leaves your shell profiles unchanged. `pinset self shell <bash|zsh|fish|powershell>` prints the PATH fragment for you to review and apply. If you choose to persist it, review the target profile and add the fragment once yourself.

Please remove private paths, project identifiers and other private information before posting. Do not include secrets, private identities or `.pinset/local/` contents.

Start with the [Pinset website](https://pinset.future-element.com) and the [main repository](https://github.com/Future-Element/pinset). Share a minimal configuration, try someone else's setup, or open an issue with reproducible steps when something fails.
