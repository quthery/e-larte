#!/usr/bin/env python3
import argparse
import sys
from pathlib import Path

import torch


def main() -> None:
    parser = argparse.ArgumentParser(description="Export real-world SwinIR-L x4 to ONNX")
    parser.add_argument("checkpoint", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--swinir-repo", type=Path, required=True)
    args = parser.parse_args()

    sys.path.insert(0, str(args.swinir_repo))
    from models.network_swinir import SwinIR

    model = SwinIR(
        upscale=4,
        in_chans=3,
        img_size=64,
        window_size=8,
        img_range=1.0,
        depths=[6] * 9,
        embed_dim=240,
        num_heads=[8] * 9,
        mlp_ratio=2,
        upsampler="nearest+conv",
        resi_connection="3conv",
    )
    checkpoint = torch.load(args.checkpoint, map_location="cpu", weights_only=True)
    model.load_state_dict(checkpoint["params_ema"], strict=True)
    model.eval()

    example = torch.zeros(1, 3, 64, 64)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with torch.inference_mode():
        torch.onnx.export(
            model,
            (example,),
            args.output,
            input_names=["image"],
            output_names=["upscaled"],
            dynamic_axes={
                "image": {2: "height", 3: "width"},
                "upscaled": {2: "upscaled_height", 3: "upscaled_width"},
            },
            opset_version=17,
            dynamo=False,
        )


if __name__ == "__main__":
    main()
