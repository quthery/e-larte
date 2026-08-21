#!/usr/bin/env python3
import argparse
import importlib.util
import sys
import types
from pathlib import Path

import torch
from timm.layers import to_2tuple, trunc_normal_


class _Registry:
    def register(self):
        return lambda value: value


def install_basicsr_import_shims() -> None:
    basicsr = types.ModuleType("basicsr")
    utils = types.ModuleType("basicsr.utils")
    registry = types.ModuleType("basicsr.utils.registry")
    archs = types.ModuleType("basicsr.archs")
    arch_util = types.ModuleType("basicsr.archs.arch_util")
    registry.ARCH_REGISTRY = _Registry()
    arch_util.to_2tuple = to_2tuple
    arch_util.trunc_normal_ = trunc_normal_
    sys.modules.update({
        "basicsr": basicsr,
        "basicsr.utils": utils,
        "basicsr.utils.registry": registry,
        "basicsr.archs": archs,
        "basicsr.archs.arch_util": arch_util,
    })


def main() -> None:
    parser = argparse.ArgumentParser(description="Export real-world HAT-GAN x4 to ONNX")
    parser.add_argument("checkpoint", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--hat-repo", type=Path, required=True)
    parser.add_argument("--input-size", type=int, default=64)
    args = parser.parse_args()

    install_basicsr_import_shims()
    module_path = args.hat_repo / "hat" / "archs" / "hat_arch.py"
    spec = importlib.util.spec_from_file_location("hat_arch", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load HAT architecture from {module_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    HAT = module.HAT

    model = HAT(
        upscale=4,
        in_chans=3,
        img_size=64,
        window_size=16,
        compress_ratio=3,
        squeeze_factor=30,
        conv_scale=0.01,
        overlap_ratio=0.5,
        img_range=1.0,
        depths=[6] * 6,
        embed_dim=180,
        num_heads=[6] * 6,
        mlp_ratio=2,
        upsampler="pixelshuffle",
        resi_connection="1conv",
    )
    checkpoint = torch.load(args.checkpoint, map_location="cpu", weights_only=True)
    model.load_state_dict(checkpoint["params_ema"], strict=True)
    model.eval()

    if args.input_size % 16 != 0:
        raise ValueError("HAT input size must be divisible by its 16-pixel attention window")
    example = torch.zeros(1, 3, args.input_size, args.input_size)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with torch.inference_mode():
        torch.onnx.export(
            model,
            (example,),
            args.output,
            input_names=["image"],
            output_names=["upscaled"],
            opset_version=17,
            dynamo=False,
        )


if __name__ == "__main__":
    main()
