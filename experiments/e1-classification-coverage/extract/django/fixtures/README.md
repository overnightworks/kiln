# Recorded analyzer output

`<application>/messages.json` is the `messages` array of `splinter-mypy`'s
`messages.json`, kept for the one source file of that application's worked
example (`inputs.md` §3.1) and nothing else; the source file sits next to it,
byte-identical to the checkout. The only edit is that the absolute corpus path
in front of each `filePath` is replaced by `<application>/`, so the fixture does
not depend on where it was recorded.

- babybuddy: `babybuddy/babybuddy` @ `ba46c49bea0694fd6b9bda728bcd281ef15f284c`, file `core/views.py` (BSD-2-Clause).
- zulip: `zulip/zulip` @ `88e91fa494beefe3d6ce1b193b97a4be315a1506`, file `zerver/actions/create_user.py` (Apache-2.0).

Recorded on 2026-10-16 with `extract.py units <owner/repo>` (which runs the
frozen command line of `inputs.md` §3 on the absolute application root) in the
pinned environment: Python 3.11.15, `splinter-mypy`
`f8057cae6c2faae19e7cf97f3026a4fbebd4f9cc`, `mypy==1.9.0`, `django==5.0.3`,
`django-stubs==4.2.7`, `django-stubs-ext==4.2.7`, `django-filter-stubs==0.1.3`,
`djangorestframework-stubs==3.14.5`, installed by `extract.py install-analyzer`.
