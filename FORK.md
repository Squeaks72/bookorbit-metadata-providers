# Squeaks72 fork

Fork of [bookorbit/bookorbit-metadata-providers](https://github.com/bookorbit/bookorbit-metadata-providers),
carrying one change to the MangaBaka provider and publishing it as
`ghcr.io/squeaks72/mangabaka-provider` (`:latest`, plus an immutable `<version>-<sha>`).

- `main` mirrors upstream (fast-forward only, never committed to). `tim` is the default branch:
  upstream plus the patches below, rebased nightly by `fork-sync.yml`; `fork-build.yml` builds the image.
- Upstream's `publish.yml` and `test.yml` stay disabled here.

## The patch: chapter-aware queries (`providers/mangabaka/src/mapping.rs`)

BookOrbit sends only a book's title as the query. For a library of single chapters that title is
`# 211` or `Blue Box - Chapter 211`, and MangaBaka's search finds nothing once the query carries extra
words. `split_volume` now first cuts off a chapter marker and everything after it (`Chapter`, `Ch.`,
`Episode`, `Ep`, `Class`, `Lesson`, `Round`, `Act`, `Issue`, `#`, and a few European spellings) and
drops its number: a chapter is not a volume, so the provider answers with the series and no
`Vol. 211`. A marker must follow a separator and have something before it, so `Escape 5` and a bare
`Chapter 92` are left alone. `Vol. N` still reads as a volume. Behaviour change from upstream: `#N`
is a chapter here, not a volume.

`VERSION` in `src/main.rs` carries a `-tim.N` suffix; bump N with every change to the provider's
answers (BookOrbit keys its cache on it). If upstream changes that line in a rebase, take its number and
keep the `-tim.N` suffix.

## New provider: GraphicAudio (`providers/graphicaudio`)

Not in upstream. Reads graphicaudio.net product pages (full-cast audiobooks, one entry per part,
cast as narrators) for a dedicated audiobook library. Published as
`ghcr.io/squeaks72/graphicaudio-provider`; `fork-build.yml` builds both providers. `VERSION` carries
`-tim.N` like MangaBaka's.

## Conflict policy

When upstream adds similar chapter handling, upstream wins: drop the patch.
