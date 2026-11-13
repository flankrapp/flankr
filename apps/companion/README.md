# flankr companion app

The companion app is flankr on your phone. It runs as an Android app
(`dev.flankr.companion`) and as a desktop window. It pairs with a flankr
daemon over an encrypted link, lists the coding agents running under herdr
on that computer, and lets you follow and answer them: each session as a
conversation of cards or as the agent's own terminal, with its questions,
approvals and finished turns raised as they happen. You can talk to it,
type, dictate, or press the terminal's own keys. Speech is recognised and
spoken on the phone itself, by models you download separately (see
[Voice models](#voice-models)).

The app is a thin front end over two crates in this repository:
[flankr-companion](../../crates/flankr-companion) (pairing, the link,
sessions, keys and turns) and [flankr-audio](../../crates/flankr-audio)
(speech in and out). What has changed, release by release, is in
[UPDATES.md](../UPDATES.md); the app's version is `version` in
[Cargo.toml](Cargo.toml).

## Use at your own risk

The app's approvals and answers reach an agent as keyboard input that the
daemon types into its terminal; see
[Use at your own risk](../../README.md#use-at-your-own-risk) in the main
README.

## Building and running

[apps/Cargo.toml](../Cargo.toml) is its own workspace, so `cargo build` at
the repository root never builds the app. Build from `apps/`:

```sh
cd apps
cargo run --release -p companion -- [--models <dir>] [--quant q4k|q4_0|q8] [--voice <name>] \
    [--pair <pairing URL, flankr1… string or data: URL>] [--keys <dir>] [--name <device name>]
```

The desktop window shows the app in an emulated phone and uses the default
audio devices. `--models` is the folder holding `phonon-31m-en` and
`stt-1b-en_fr` (default `models/`, which is gitignored). Keys are kept in
the login Keychain on macOS, or in files under `--keys`.

### Android

```sh
apps/companion/scripts/android.sh [--install] [-- <extra cargo args>]
```

This builds the Rust library for `aarch64-linux-android`, writes the Gradle
project from `build.rs`, and packages a debug APK (arm64-v8a only).
`--install` installs and launches it with `adb`. It needs the
`aarch64-linux-android` Rust target, an Android SDK with an NDK, Gradle 9
and JDK 17 or 21; `ANDROID_HOME`, `ANDROID_NDK_ROOT` and `JAVA_HOME`
override discovery.

The scripts below copy files into the app's data folder on the phone with
`adb` and `run-as`, so the debug APK must be installed first. `<models>` is
`$FLANKR_MODELS`, or `apps/companion/models`.

| Script | What it copies | Where on the phone |
|---|---|---|
| `scripts/push-model.sh [<model dir>]` | The TTS model and its voices, from `<models>/phonon-31m-en` | `files/phonon-31m-en` |
| `scripts/push-stt.sh [<model dir>]` | The STT model (`model.q4k.gguf`, `config.json`, `tokenizer.model`; about 1 GB), from `<models>/stt-1b-en_fr`. `STT_QUANTS` picks other quantizations, `STAGE` the staging folder on the phone. | `files/stt-1b-en_fr` |
| `scripts/push-vocab.sh <vocab file>` | `vocab.txt`, the words the speech recogniser is boosted towards. The app rewrites it from the daemon's vocabulary. | `files/vocab.txt` |
| `scripts/push-pair.sh '<pairing text>'` | What `flankr pair` printed, as `pair.txt`; the app pairs with it at its next start and deletes it. | `files/pair.txt` |

## Voice models

The voice models are not part of flankr. They are not in this repository
and not in the APK. You download them yourself from Hugging Face onto your
computer, into `<models>`, and then push them to the phone with the scripts
above; the desktop window reads them from `--models`. Each is under its own
licence, set by its authors:

| Model | Model card | Licence | Folder and files the app loads |
|---|---|---|---|
| Speech-to-text: `stt-1b-en_fr`, by Kyutai | <https://huggingface.co/kyutai/stt-1b-en_fr-candle> | CC-BY-4.0 | `stt-1b-en_fr/`: `model.q4k.gguf`, `config.json`, `tokenizer.model` |
| Text-to-speech: Phonon 31m (`phonon-31m-en`), by Gradium | <https://huggingface.co/gradium/phonon-31m-en> | Apache-2.0 | `phonon-31m-en/`: `model.q8.gguf`, `config.json`, `tokenizer.json`, `default-voice.safetensors` |
| Voices for Phonon, by Gradium | <https://huggingface.co/gradium/phonon-voices> | Apache-2.0 | `phonon-31m-en/voices/*.safetensors` |

**Speech-to-text.** The model card states: "Model weights are licensed
under CC-BY 4.0", and the repository is tagged `cc-by-4.0`. The card asks
for no particular form of credit, so the credit is:

> `stt-1b-en_fr` by Kyutai (<https://huggingface.co/kyutai/stt-1b-en_fr-candle>),
> licensed under CC BY 4.0 (<https://creativecommons.org/licenses/by/4.0/>).
> Changed for this app: the language model is quantized to q4k and stored
> with the encoder half of the repository's Mimi checkpoint in one file,
> `model.q4k.gguf`.

The repository publishes `model.safetensors` and the Mimi checkpoint, not
`model.q4k.gguf`, which is made from them. `config.json` and
`tokenizer.model` (`tokenizer_en_fr_audio_8000.model` in the repository)
are used as published; the card does not state their licence separately
from the weights.

**Text-to-speech and voices.** The Phonon card states: "The weights,
tokenizer and voices in this repository are released under Apache 2.0", and
credits its base: "Phonon builds on Pocket TTS and the Mimi codec,
developed by Kyutai." The voices repository is tagged `apache-2.0`. The app
uses both as published.

## Licence

The app is part of flankr and is licensed under the Apache License, Version
2.0: [LICENSE](../../LICENSE). The launcher icon is flankr's own, made by
[icon/make_icon.py](icon/make_icon.py) from an AI-generated source image.

## Third-party licences and notices

### Fonts

The app bundles these fonts, enabled by the features
`font-apache-roboto-regular`, `font-apache-roboto-bold`,
`font-apache-roboto-italic` and `fonts-ofl-terminal` in
[Cargo.toml](Cargo.toml). The app contains subset font files and the
glyph atlases made from them. The licence files under `licenses/` are
word-for-word copies of the notices that come with these font files: each
lists the fonts, their copyright lines and their changes, followed by the
full licence text.

| Font | Copyright | Licence | Licence text |
|---|---|---|---|
| Roboto Regular, Bold and Italic (subset) | Copyright 2011 Google Inc. All Rights Reserved. | Apache-2.0 | [licenses/FONTS-LICENSE-APACHE.txt](licenses/FONTS-LICENSE-APACHE.txt) |
| Noto Sans Mono Regular and Bold (static instances, subset) | Copyright 2022 The Noto Project Authors (https://github.com/notofonts/latin-greek-cyrillic). No Reserved Font Name. | OFL-1.1 | [licenses/FONTS-LICENSE-OFL.txt](licenses/FONTS-LICENSE-OFL.txt) |
| Libmsdf Terminal Symbols (a Modified Version merged from five Noto faces, each with no Reserved Font Name) | Noto Sans Mono, Noto Sans Symbols 2 and Noto Sans Symbols: Copyright 2022 The Noto Project Authors. Noto Sans Math: Copyright 2022 Google LLC. Noto Emoji: Copyright 2013 Google LLC. | OFL-1.1 | [licenses/FONTS-LICENSE-OFL.txt](licenses/FONTS-LICENSE-OFL.txt) |

### Libraries

The app links these libraries, beyond flankr's own crates:

| Crate | Licence |
|---|---|
| libmorph | MIT OR Apache-2.0 |
| libmobile | MIT OR Apache-2.0 |
| libdevice | MIT OR Apache-2.0 |
| libvoicexn | MIT OR Apache-2.0 |
| libmsdf | MIT OR Apache-2.0 |
| libcontrols | MIT |
| xn | MIT OR Apache-2.0 |
| xn-moshi | MIT OR Apache-2.0 |
| ptts | MIT OR Apache-2.0 |
| earshot | MIT OR Apache-2.0 |
| tokenizers | Apache-2.0 |
| rqrr | (MIT OR Apache-2.0) AND ISC |

The rest of the app's crates.io dependencies, in
[apps/Cargo.lock](../Cargo.lock), are almost all under MIT, Apache-2.0 or
both, with a few under BSD, ISC, Zlib, Unicode-3.0 and similar permissive
licences. `cargo metadata --format-version 1`, run in `apps/`, lists each
one's licence.

### herdr

The app contains no herdr code. It does not depend on `libherdr_term`,
directly or through its path dependencies, and no `libherdr` crate is in
[apps/Cargo.lock](../Cargo.lock).
