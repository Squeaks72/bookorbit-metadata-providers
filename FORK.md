# Squeaks72 fork

Fork of [bookorbit/bookorbit-metadata-providers](https://github.com/bookorbit/bookorbit-metadata-providers),
carrying one added provider, GraphicAudio, published as `ghcr.io/squeaks72/graphicaudio-provider`
(`:latest`, plus an immutable `<version>-<sha>`). The MangaBaka provider is upstream's, unmodified: run
`ghcr.io/bookorbit/mangabaka-provider`.

- `main` mirrors upstream (fast-forward only, never committed to). `tim` is the default branch:
  upstream plus the patches below, rebased nightly by `fork-sync.yml`; `fork-build.yml` builds the image.
- Upstream's `publish.yml` and `test.yml` stay disabled here.

## New provider: GraphicAudio (`providers/graphicaudio`)

Not in upstream. Reads graphicaudio.net product pages (full-cast audiobooks, one entry per part,
cast as narrators) for a dedicated audiobook library. Published as
`ghcr.io/squeaks72/graphicaudio-provider`; `fork-build.yml` builds both providers. `VERSION` carries
a `-tim.N` suffix; bump N with every change to the provider's answers (BookOrbit keys its cache on it).

## Conflict policy

When upstream adds a provider that overlaps GraphicAudio, upstream wins: drop ours.
