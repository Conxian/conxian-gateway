# TypeScript 7 migration

## Support definition

This repository uses TypeScript 7.0.2 as the native compiler for every
TypeScript workspace. Typechecking and package builds invoke the `tsc` binary
provided by the `@typescript/native` npm alias.

The TypeScript 7 package is intentionally not used as a programmatic compiler
API by the current Next.js and typescript-eslint toolchain. TypeScript 7.0 does
not expose the legacy API, while the pinned tooling still imports the package
named `typescript`. The repository therefore uses the official TypeScript
6.0 compatibility package only under that name.

As verified on July 21, 2026, the latest stable `typescript-eslint` release is
8.65.0 and declares a TypeScript peer range of `>=4.8.4 <6.1.0`. The latest
stable Next.js and `eslint-config-next` releases are 16.2.10, and that Next.js
release still imports `typescript/lib/typescript.js`. No newer stable package
combination was available that removed this bridge requirement.

ESLint is pinned to the 9.x line because the stable React/import/JSX plugins
bundled by `eslint-config-next@16.2.10` declare support through ESLint 9, not
ESLint 10. This keeps the lint gate executable without changing the compiler
split.

This is a compatibility bridge, not a second typechecking path: CI verifies
that workspace `tsc` commands report TypeScript 7.0.2 and that the control
plane's `tsc6` compatibility executable is present only for legacy tooling.
The package alias is pinned to `@typescript/typescript6@6.0.2`; its `tsc6`
executable reports the underlying `typescript@6.0.3` compiler version.

## Workspace compatibility matrix

| Workspace | Native typecheck/build compiler | Legacy API package | Reason |
| --- | --- | --- | --- |
| repository root | TypeScript 7.0.2 via `@typescript/native` | TypeScript 6.0 compatibility alias | Root tooling can resolve `typescript` while native CLI remains `tsc`. |
| `apps/control-plane` | TypeScript 7.0.2 via `@typescript/native` | `@typescript/typescript6` 6.0.2 aliased as `typescript` | Next.js 16.2.10 imports `typescript/lib/typescript.js`; typescript-eslint 8.x requires the TypeScript 6 API range. |
| `packages/client-sdk` | TypeScript 7.0.2 via `@typescript/native` | none | No compiler API consumer. |
| `packages/schemas` | TypeScript 7.0.2 via `@typescript/native` | none | No compiler API consumer. |
| `examples/developer-sandbox` | TypeScript 7.0.2 via `@typescript/native` | none | No compiler API consumer. |

The aliases are declared explicitly in each workspace rather than relying on
hoisting. This keeps the compiler used by each workspace reproducible.

## Official side-by-side installation pattern

The TypeScript team documents the transition pattern as an npm alias for the
native compiler alongside `typescript` aliased to `@typescript/typescript6`:

```json
{
  "devDependencies": {
    "@typescript/native": "npm:typescript@^7.0.2",
    "typescript": "npm:@typescript/typescript6@^6.0.2"
  }
}
```

This repository pins the currently selected releases (`7.0.2` and `6.0.2`)
inside that same alias pattern so the lockfile and CI compiler checks remain
deterministic. The compatibility package supplies `tsc6`; the native alias
supplies `tsc`.

## Configuration changes

TypeScript 6 deprecates `baseUrl`, and TypeScript 7 removes it. The control
plane now keeps its `@/*` mapping with an explicit `./` path and no
`baseUrl`. Other repository TypeScript configurations contain no removed
TypeScript 7 options.

The control plane has an explicit `typecheck` script that invokes TypeScript 7
directly. Its Next.js build and ESLint configuration continue to resolve the
TypeScript 6 compatibility package until those tools publish stable support
for the TypeScript 7 package API.

## CI acceptance gates

Node CI must pass all of the following:

1. Frozen dependency installation: `pnpm install --frozen-lockfile`.
2. Compiler split checks: every workspace `tsc` reports `Version 7.0.2`, and
   only the control-plane `tsc6` executable reports its underlying
   `typescript@6.0.3` version; the compatibility package remains pinned at
   `@typescript/typescript6@6.0.2`.
3. Native typechecking: `pnpm typecheck`.
4. Linting: `pnpm lint`.
5. Workspace builds: `pnpm build`.
6. Workspace tests: `pnpm test`.

No build-error bypass, skipped typecheck, or compiler-version mismatch is
accepted as a substitute for these gates.

## Bridge removal conditions

Remove the TypeScript 6 compatibility alias only after all of the following
are verified against released package metadata and CI:

- Next.js no longer imports `typescript/lib/typescript.js` and its supported
  compiler path works with TypeScript 7.
- The stable `typescript-eslint` packages accept and use the TypeScript 7
  compiler/package API without the compatibility alias.
- The control-plane build and lint pass with the TypeScript 7 package named
  `typescript` and no `tsc6` binary.
- The repository-wide compiler audit finds no remaining legacy API importer.

## Canonical sources

The package names and side-by-side installation pattern come from the official
TypeScript 7 announcement and TypeScript 6 migration notes. Package versions
and executable metadata were checked against npm before this migration was
implemented.

```text
https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/
https://www.typescriptlang.org/docs/handbook/release-notes/typescript-6-0.html
https://www.npmjs.com/package/typescript
https://www.npmjs.com/package/@typescript/typescript6
https://www.npmjs.com/package/typescript-eslint
https://www.npmjs.com/package/next
```
