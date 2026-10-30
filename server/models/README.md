# Models

## beat-this-small0.onnx

Beat this!'s small beat and downbeat model (decision 0049, amended), which Cratarr runs on new music through tract:
`app::beat_this`. From CPJKU's `small0` checkpoint (Foscarin, Schlüter, Widmer, "Beat this! Accurate beat tracking
without DBN postprocessing", ISMIR 2024; https://github.com/CPJKU/beat_this), MIT licensed: see `LICENSE-beat-this`.

Exported with `beat_this` 1.1.0 and PyTorch, opset 17, its time axis free (a short track is one piece shorter than
1,500 frames):

```python
model = beat_this.inference.load_model('small0', device='cpu').eval()
# a wrapper returning (out['beat'], out['downbeat'])
torch.onnx.export(wrapper, (spect[:1500].unsqueeze(0),), 'beat-this-small0.onnx', input_names=['spect'],
                  output_names=['beat', 'downbeat'], opset_version=17, dynamo=False,
                  dynamic_axes={'spect': {1: 'time'}, 'beat': {1: 'time'}, 'downbeat': {1: 'time'}})
```

On a real track, ONNX Runtime gives PyTorch's logits to 6e-5 at 1,500 frames and 3e-5 at 812 (2026-10-19).
SHA-256 `fccda1227c1f605b5b0373a707e8d2a6ea82da01641e8194282f9813530fd89a`.
