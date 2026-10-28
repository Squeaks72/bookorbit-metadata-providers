# GraphicAudio provider

Metadata for [GraphicAudio](https://www.graphicaudio.net) dramatized (full-cast) audiobooks, read
from graphicaudio.net's own product pages. Point a **separate audiobook library** at it so these
recordings stay beside, not merged with, standard single-narrator audiobooks.

- **One entry per part.** GraphicAudio splits a book into parts, so `The Final Empire` is three
  products. Each answers as `The Final Empire (1 of 3)`, series `Mistborn`, sequence `1.1`
  (`4.5` is part 5 of book 4, so parts sort between books).
- **Cast as narrators** (the page's *Starring* credits), author, release date, running time
  (`duration`, seconds), ISBN-13, genre, description and cover.
- **Query handling.** Trailing `(2 of 3)`, `Part 2`, `[GraphicAudio]` and `Dramatized Adaptation` are
  stripped for the search and used to rank the right part first.
- Search is by title/series only; the site has no ISBN or ASIN search.
- Answers nothing for `book` and `comic` searches.

| Variable | Default | |
|---|---|---|
| `PORT` | `8080` | |
| `REQUEST_INTERVAL_MS` | `750` | gap between requests to graphicaudio.net |
| `AUTH_TOKEN` | unset | require `Authorization: <token>` |
| `GRAPHICAUDIO_URL` | `https://www.graphicaudio.net` | |
| `USER_AGENT` | `bookorbit-graphicaudio-provider/<version>` | |

Product pages are cached for a day. Inspired by
[graphicaudio_scraper](https://github.com/binyaminyblatt/graphicaudio_scraper); not affiliated with
GraphicAudio.
