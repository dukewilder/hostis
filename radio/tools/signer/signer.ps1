# Pyrate Radio signer. Double-click "Sign Pyrate Radio.bat" to open it.
# The words go from this window straight into release.exe and are never written to disk.
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe  = Join-Path $here "release.exe"
$cert = Join-Path $here "android-cert.der"

$black = [System.Drawing.Color]::FromArgb(10, 10, 10)
$panel = [System.Drawing.Color]::FromArgb(24, 24, 24)
$bone  = [System.Drawing.Color]::FromArgb(237, 228, 206)
$dim   = [System.Drawing.Color]::FromArgb(150, 145, 132)
$red   = [System.Drawing.Color]::FromArgb(197, 40, 40)

function Find-Apk {
  Get-ChildItem -Path $here -Filter "pyrate-radio-*-to-sign.apk" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
}

$form = New-Object System.Windows.Forms.Form
$form.Text = "Sign Pyrate Radio"
$form.Size = New-Object System.Drawing.Size(560, 390)
$form.StartPosition = "CenterScreen"
$form.FormBorderStyle = "FixedDialog"
$form.MaximizeBox = $false
$form.BackColor = $black
$form.ForeColor = $bone
$form.Font = New-Object System.Drawing.Font("Georgia", 11)

$title = New-Object System.Windows.Forms.Label
$title.Text = "SIGN PYRATE RADIO"
$title.Font = New-Object System.Drawing.Font("Georgia", 16, [System.Drawing.FontStyle]::Bold)
$title.AutoSize = $true
$title.Location = New-Object System.Drawing.Point(28, 22)
$form.Controls.Add($title)

$what = New-Object System.Windows.Forms.Label
$what.AutoSize = $false
$what.Size = New-Object System.Drawing.Size(500, 44)
$what.Location = New-Object System.Drawing.Point(30, 64)
$what.ForeColor = $dim
$form.Controls.Add($what)

$ask = New-Object System.Windows.Forms.Label
$ask.Text = "Type your 24 Android words, with a space between each:"
$ask.AutoSize = $true
$ask.Location = New-Object System.Drawing.Point(30, 118)
$form.Controls.Add($ask)

$words = New-Object System.Windows.Forms.TextBox
$words.Location = New-Object System.Drawing.Point(30, 146)
$words.Size = New-Object System.Drawing.Size(490, 28)
$words.BackColor = $panel
$words.ForeColor = $bone
$words.BorderStyle = "FixedSingle"
$words.UseSystemPasswordChar = $true
$form.Controls.Add($words)

$show = New-Object System.Windows.Forms.CheckBox
$show.Text = "Show the words while I type"
$show.AutoSize = $true
$show.ForeColor = $dim
$show.Location = New-Object System.Drawing.Point(30, 182)
$show.Add_CheckedChanged({ $words.UseSystemPasswordChar = -not $show.Checked })
$form.Controls.Add($show)

$go = New-Object System.Windows.Forms.Button
$go.Text = "SIGN THE APP"
$go.Size = New-Object System.Drawing.Size(200, 44)
$go.Location = New-Object System.Drawing.Point(30, 222)
$go.FlatStyle = "Flat"
$go.FlatAppearance.BorderColor = $red
$go.BackColor = [System.Drawing.Color]::FromArgb(40, 10, 10)
$go.ForeColor = $bone
$form.Controls.Add($go)
$form.AcceptButton = $go

$note = New-Object System.Windows.Forms.Label
$note.AutoSize = $false
$note.Size = New-Object System.Drawing.Size(500, 70)
$note.Location = New-Object System.Drawing.Point(30, 280)
$note.ForeColor = $dim
$note.Text = "Your words stay in this window. Nothing is saved or sent anywhere."
$form.Controls.Add($note)

function Refresh-Apk {
  $script:apk = Find-Apk
  if ($script:apk) {
    $what.Text = "Ready to sign: " + $script:apk.Name
    $go.Enabled = $true
  } else {
    $what.Text = "No app to sign here yet. Put the file Claude sent you (it ends in -to-sign.apk) in this folder, then close and reopen this window."
    $go.Enabled = $false
  }
}
Refresh-Apk

if (-not (Test-Path $exe) -or -not (Test-Path $cert)) {
  $what.Text = "release.exe or android-cert.der is missing from this folder. Ask Claude for them."
  $go.Enabled = $false
}

$go.Add_Click({
  $text = ($words.Text -replace '\s+', ' ').Trim().ToLower()
  $count = if ($text) { ($text -split ' ').Count } else { 0 }
  if ($count -ne 24) {
    $note.ForeColor = $red
    $note.Text = "That is $count words. It needs all 24, in order, with a space between each."
    return
  }
  $version = ($script:apk.Name -replace '^pyrate-radio-', '' -replace '-to-sign\.apk$', '')
  $out = Join-Path $here ("Pyrate Radio " + $version + ".apk")
  $go.Enabled = $false
  $note.ForeColor = $dim
  $note.Text = "Signing. This takes a few seconds."
  $form.Refresh()

  $p = New-Object System.Diagnostics.Process
  $p.StartInfo.FileName = $exe
  $p.StartInfo.Arguments = "sign-apk - `"$cert`" `"$($script:apk.FullName)`" `"$out`""
  $p.StartInfo.UseShellExecute = $false
  $p.StartInfo.CreateNoWindow = $true
  $p.StartInfo.RedirectStandardInput = $true
  $p.StartInfo.RedirectStandardOutput = $true
  $p.StartInfo.RedirectStandardError = $true
  $p.Start() | Out-Null
  $p.StandardInput.Write($text)
  $p.StandardInput.Close()
  $err = $p.StandardError.ReadToEnd()
  $p.WaitForExit()
  $words.Text = ""
  $text = $null

  if ($p.ExitCode -eq 0 -and (Test-Path $out)) {
    $note.ForeColor = $bone
    $note.Text = "Done. The signed app is `"Pyrate Radio $version.apk`" in this folder. Send that file to Claude."
    Start-Process explorer.exe "/select,`"$out`""
  } else {
    $note.ForeColor = $red
    if ($err -match "do not match") {
      $note.Text = "Those words are not your Android words. Check them against your paper and try again."
    } elseif ($err -match "phrase|word|checksum") {
      $note.Text = "One of those words is misspelt or out of order. Check them against your paper and try again."
    } else {
      $note.Text = "It did not sign. Tell Claude this: " + ($err -split "`n")[0]
    }
    $go.Enabled = $true
  }
})

[void]$form.ShowDialog()
