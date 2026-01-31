# Download Parakeet Model Script
# This script downloads the sherpa-onnx parakeet model if it doesn't exist

$MODEL_DIR = "data/parakeet_model"
$MODEL_URL = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2"
$MODEL_ARCHIVE = "parakeet-model.tar.bz2"

Write-Host "Downloading Parakeet model..."

# Clean existing model directory and recreate
if (Test-Path $MODEL_DIR) {
    Remove-Item -Recurse -Force $MODEL_DIR
}
New-Item -ItemType Directory -Force -Path $MODEL_DIR | Out-Null

# Download the model archive
try {
    Write-Host "Downloading from $MODEL_URL ..."
    Invoke-WebRequest -Uri $MODEL_URL -OutFile $MODEL_ARCHIVE -UseBasicParsing
} catch {
    Write-Error "Failed to download model: $_"
    exit 1
}

Write-Host "Extracting model..."

# Check if tar is available (Windows 10+ has it built-in)
try {
    tar -xjf $MODEL_ARCHIVE -C $MODEL_DIR --strip-components=1
} catch {
    Write-Error "Failed to extract model. Please install tar or 7-zip."
    Write-Error "You can also manually extract $MODEL_ARCHIVE to $MODEL_DIR"
    exit 1
}

# Clean up
Remove-Item $MODEL_ARCHIVE -Force

Write-Host "Model downloaded and extracted to $MODEL_DIR"
