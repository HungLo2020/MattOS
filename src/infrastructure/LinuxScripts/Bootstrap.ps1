[CmdletBinding()]
param()

# Bootstrap the project-local Python environment on native Windows.
# It installs Python through Winget only when a supported interpreter is absent.
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSCommandPath
$VenvPython = Join-Path $ProjectRoot ".venv\Scripts\python.exe"
$RequirementsFile = Join-Path $ProjectRoot "requirements.txt"
$script:PythonCommand = ""
$script:PythonArguments = @()

function Write-Status([string]$Message) {
    Write-Host "[Bootstrap] $Message"
}

function Stop-Bootstrap([string]$Message) {
    throw "[Bootstrap] ERROR: $Message"
}

function Test-PythonCommand([string]$Command, [string[]]$Arguments) {
    try {
        & $Command @Arguments -c "import sys; raise SystemExit(0 if sys.version_info >= (3, 10) else 1)" | Out-Null
        return $LASTEXITCODE -eq 0
    } catch {
        return $false
    }
}

function Find-SupportedPython {
    if (Get-Command py -ErrorAction SilentlyContinue) {
        if (Test-PythonCommand "py" @("-3")) {
            $script:PythonCommand = "py"
            $script:PythonArguments = @("-3")
            return $true
        }
    }

    if (Get-Command python -ErrorAction SilentlyContinue) {
        if (Test-PythonCommand "python" @()) {
            $script:PythonCommand = "python"
            $script:PythonArguments = @()
            return $true
        }
    }

    return $false
}

function Install-Python {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Stop-Bootstrap "Python 3.10+ is required. Install Python from https://www.python.org/downloads/windows/ or install Microsoft App Installer to provide winget, then rerun this script."
    }

    Write-Status "Installing Python 3.12 with winget."
    & winget install --id Python.Python.3.12 --exact --source winget --accept-package-agreements --accept-source-agreements
    if ($LASTEXITCODE -ne 0) {
        Stop-Bootstrap "winget could not install Python. Install Python 3.10+ from https://www.python.org/downloads/windows/, reopen PowerShell, and rerun this script."
    }

    if (-not (Find-SupportedPython)) {
        Stop-Bootstrap "Python was installed, but this PowerShell session cannot find it yet. Close and reopen PowerShell, then rerun this script."
    }
}

function Invoke-ProjectPython([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments) {
    & $script:PythonCommand @script:PythonArguments @Arguments
}

if (-not (Find-SupportedPython)) {
    Install-Python
}

Write-Status "Using Python command: $script:PythonCommand $($script:PythonArguments -join ' ')"
if (-not (Test-Path $VenvPython)) {
    Write-Status "Creating virtual environment: $ProjectRoot\.venv"
    Invoke-ProjectPython -m venv (Join-Path $ProjectRoot ".venv")
}

if (-not (Test-Path $VenvPython)) {
    Stop-Bootstrap "The virtual environment was not created at $VenvPython."
}

if (Test-Path $RequirementsFile) {
    Write-Status "Installing project Python requirements."
    & $VenvPython -m pip install --requirement $RequirementsFile
}

Write-Status "Python bootstrap complete."
Write-Status "Interpreter: $VenvPython"