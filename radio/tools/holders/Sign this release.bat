@echo off
rem Opens the Pyrate Radio release signing window.
powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0holder-signer.ps1"
