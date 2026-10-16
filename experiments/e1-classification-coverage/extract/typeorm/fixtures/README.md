# Recorded analyzer output

`<application>/messages.json` is the `messages` array of `splinter-eslint`'s
`messages.json`, kept for the one source file of that application's worked
example (`inputs.md` §3.1) and nothing else; the source file sits below the
application, byte-identical to the checkout, at the path the units report.
The analyzer reports paths relative to the directory it was given, so
EPGStation's messages name `model/db/RecordedDB.ts` while its file lives at
`src/model/db/RecordedDB.ts`.

- epgstation: `l3tnun/EPGStation` @ `c0e201b83231dfed73e5bea1b2ec54866fcfbbba`, analyzed in `src` (the directory of its nearest `tsconfig.json`), file `src/model/db/RecordedDB.ts` (MIT).
- cool-admin-midway: `cool-team-official/cool-admin-midway` @ `edc13838439a0d6469abfe6263f51f2c987284bf`, analyzed at its root, file `src/modules/cloud/service/db.ts` (MIT).

Recorded on 2026-10-16 with `extract.mjs units <owner/repo>` (which runs the
frozen command line of `inputs.md` §3) in the pinned environment: Node 22,
`@ctring/splinter-eslint@1.0.9` with `@ctring/eslint-plugin-typeorm@1.0.1`
and `typescript@5.4.3`, installed by `extract.mjs install-analyzer`, after
`extract.mjs install-dependencies <owner/repo>` in the checkout.
