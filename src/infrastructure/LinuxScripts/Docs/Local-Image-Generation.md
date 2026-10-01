# Local image generation

The managed `stable-diffusion` workload runs Stable Diffusion WebUI Forge on the existing `ai-stack` Docker network. Forge remains reachable at `http://localhost:7861`; Open WebUI sends image requests to its model-aware API bridge at `http://automatic1111:7862/`. The bridge selects the requested checkpoint and applies model-specific Forge components, CPU offloading, sampler and guidance settings. Open WebUI lists the Forge checkpoints as image-generation models, so select a model in the chat image-generation controls.

## Installed models and files

All model files live below `~/.automatic1111/models/` and are bind-mounted into Forge. Downloads are size- and SHA-256-verified before use.

| Use | File | Location | Source |
| --- | --- | --- | --- |
| Photorealism / people | `RealVisXL_V5.0_fp16.safetensors` (full FP16, not Lightning) | `Stable-diffusion/` | [Original SG161222 model](https://huggingface.co/SG161222/RealVisXL_V5.0) |
| General / concept art | `flux1-dev-Q5_1.gguf` | `Stable-diffusion/` | [City96 GGUF conversion of BFL FLUX.1-dev](https://huggingface.co/city96/FLUX.1-dev-gguf) |
| FLUX VAE | `ae.safetensors` | `VAE/` | Extracted weights, SHA-256 checked against the official FLUX VAE ([public verified copy](https://huggingface.co/flux-safetensors/flux-safetensors)) |
| FLUX CLIP-L | `clip_l.safetensors` | `text_encoder/` | [ComfyUI maintainer’s encoder repository](https://huggingface.co/comfyanonymous/flux_text_encoders) |
| FLUX T5-XXL | `t5xxl_fp16.safetensors` | `text_encoder/` | [ComfyUI maintainer’s encoder repository](https://huggingface.co/comfyanonymous/flux_text_encoders) |

The FLUX Q5_1 file is about 9.0 GB; full-FP16 T5-XXL is about 9.8 GB. Forge loads the VAE and text encoders with FLUX and keeps CPU as its swap location. With 8 GB VRAM, Forge is configured with 2 GB inference memory and offloads model weights through system RAM. This favors output fidelity and stability over speed. The exact observed GPU in this host is an RTX 5060 with 8 GiB VRAM (the deployment target was described as RTX 5070); settings are chosen for the shared 8 GB capacity. Ollama unloads its model after a request so it does not hold VRAM while image generation runs.

## Recommended starting settings

The API bridge applies these per model even when requests originate in Open WebUI:

| Model | Size | Steps | Sampler / schedule | Guidance | Negative prompt |
| --- | --- | ---: | --- | --- | --- |
| RealVisXL V5.0 FP16 | 1024x1024 | 32 | DPM++ SDE / Karras | CFG 5.5 | Uses the model author's recommended anatomy/face negative prompt when none is supplied |
| FLUX.1-dev Q5_1 | 1024x1024 | 30 | Euler / Simple | CFG 1, distilled CFG 3.5 | Empty; FLUX distilled guidance is used instead |

RealVisXL V5.0 is aimed at photorealism and its model card explicitly describes SFW and NSFW output. FLUX.1-dev is a general prompt-following/concept-art model; its original license restrictions also apply to the Q5_1 quantized copy. [RealVisXL model card](https://huggingface.co/SG161222/RealVisXL_V5.0), [FLUX.1-dev license/model](https://huggingface.co/black-forest-labs/FLUX.1-dev).

## Verification record

Verified on 2026-09-24 on the local 8 GiB GPU:

- The Open WebUI container is configured to call `http://automatic1111:7862/`. Its model-discovery endpoint successfully returned RealVisXL, FLUX Q5_1, and the retained DreamShaper checkpoints. Open WebUI's image routes require an authenticated user; the matching A1111-compatible model-selection and `/txt2img` calls were exercised through the configured bridge from the Open WebUI container.
- RealVisXL generated a valid 1024x1024 PNG in 38.7 seconds at 32 steps. Forge loaded the FP16 XL checkpoint and completed without CUDA OOM or missing-model errors. Its log reported about 4.9 GB for the denoiser and 1.56 GB for the text encoder before automatic memory management.
- FLUX.1-dev Q5_1 generated a valid 1024x1024 PNG in 142.4 seconds at 30 steps. Forge confirmed the GGUF Q5_1 weights and all three additional modules (VAE, CLIP-L, T5-XXL FP16); it completed without CUDA OOM or missing-model errors. Sampled GPU use peaked around 7.2 of 8.15 GB. The Forge container used about 18.5 GiB RAM after inference; the host had about 36 GiB available out of 62 GiB. Forge logs confirm CPU swapping for both the T5 encoder and denoiser.
- DreamShaper checkpoints remain in `Stable-diffusion/` as rollback options. RealVisXL is the Forge launch default; choose FLUX in the Open WebUI image-generation model selector for concept art and hard-surface work.
