# Super Puper Image Scaler

Desktop image upscaling with ONNX Runtime and Vulkan through Real-ESRGAN NCNN.

## Enable Vulkan

Install the portable runtime and its native models, then restart the app:

```sh
python3 scripts/install_ncnn.py
cargo run
```

On Windows, `py -3 scripts/install_ncnn.py` also works. The installer requires
Python 3.9 or newer and downloads the official
[Real-ESRGAN 20220424 bundle](https://github.com/xinntao/Real-ESRGAN/releases/tag/v0.2.5.0).
It installs into `tools/runtimes/ncnn`; downloaded binaries and weights are ignored
by Git. Existing installations are never overwritten.

Select a model ending in **Vulkan**, then **Vulkan (NCNN)** in the backend menu.
The bundled Real-ESRGAN x4plus and x4plus-anime models upscale by 4×;
AnimeVideo v3 provides separate 2×, 3× and 4× models. No ONNX import is needed.
The runtime automatically selects a GPU and tile size. A working Vulkan GPU
driver is required; the macOS universal binary includes MoltenVK for Apple and
Intel Macs. Runtime errors appear in the app with the native diagnostic message.

Imported Real-ESRGAN-x4plus ONNX models can also use the installed NCNN x4plus
weights. Other imported ONNX models use ONNX Runtime; arbitrary ONNX graphs and
HAT models are not converted to NCNN automatically.

## Portable and custom installations

An existing downloaded ZIP can be installed without network access:

```sh
python3 scripts/install_ncnn.py --archive /path/to/realesrgan-ncnn-vulkan-20220424-macos.zip
```

Use `--destination` for a custom directory and `--platform macos|ubuntu|windows`
to prepare a bundle for another OS. Official prebuilt Linux and Windows bundles
are x86-64; other architectures require a source build.

Runtime discovery checks these locations in order:

1. `SUPER_PUPER_NCNN_BINARY` and `SUPER_PUPER_NCNN_MODELS` (set both to override discovery).
2. An `ncnn` directory beside the app executable.
3. `Contents/Resources/ncnn` in a macOS app bundle.
4. `runtimes/ncnn` in the app's local data directory.
5. This checkout's `tools/runtimes/ncnn`.
6. Directories on `PATH` containing the executable and an adjacent `models` directory.

Each installation contains `realesrgan-ncnn-vulkan` (`.exe` on Windows) and a
`models` folder with matching `.param`/`.bin` pairs. Unix binaries must be executable.
For packaging, copy the installed directory to the appropriate portable/app-bundle
location and retain upstream notices. Vulkan is offered only for model pairs that
are actually present.

## Checks

```sh
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The GPU smoke test is opt-in so CI can run without GPU hardware:

```sh
cargo test bundled_runtime_upscales_a_small_image -- --ignored --nocapture
```
