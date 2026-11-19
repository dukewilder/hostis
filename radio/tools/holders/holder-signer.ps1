# Pyrate Radio release signer, for the three release key holders.
# Double-click "Sign this release.bat" inside the extracted release folder.
# The words go from this window straight into release.exe and are never written to disk.
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe  = Join-Path $here "release.exe"

$black = [System.Drawing.Color]::FromArgb(10, 10, 10)
$panel = [System.Drawing.Color]::FromArgb(24, 24, 24)
$bone  = [System.Drawing.Color]::FromArgb(237, 228, 206)
$dim   = [System.Drawing.Color]::FromArgb(150, 145, 132)
$red   = [System.Drawing.Color]::FromArgb(197, 40, 40)

function Run-Release([string]$arguments, [string]$stdin) {
  $p = New-Object System.Diagnostics.Process
  $p.StartInfo.FileName = $exe
  $p.StartInfo.Arguments = $arguments
  $p.StartInfo.WorkingDirectory = $here
  $p.StartInfo.UseShellExecute = $false
  $p.StartInfo.CreateNoWindow = $true
  $p.StartInfo.RedirectStandardInput = $true
  $p.StartInfo.RedirectStandardOutput = $true
  $p.StartInfo.RedirectStandardError = $true
  $p.Start() | Out-Null
  if ($stdin) { $p.StandardInput.Write($stdin) }
  $p.StandardInput.Close()
  $out = $p.StandardOutput.ReadToEnd()
  $err = $p.StandardError.ReadToEnd()
  $p.WaitForExit()
  return @{ Code = $p.ExitCode; Out = $out; Err = $err }
}

$form = New-Object System.Windows.Forms.Form
$form.Text = "Sign this release"
$form.Size = New-Object System.Drawing.Size(620, 600)
$form.StartPosition = "CenterScreen"
$form.FormBorderStyle = "FixedDialog"
$form.MaximizeBox = $false
$form.BackColor = $black
$form.ForeColor = $bone
$form.Font = New-Object System.Drawing.Font("Georgia", 11)

$title = New-Object System.Windows.Forms.Label
$title.Text = "SIGN THIS RELEASE"
$title.Font = New-Object System.Drawing.Font("Georgia", 16, [System.Drawing.FontStyle]::Bold)
$title.AutoSize = $true
$title.Location = New-Object System.Drawing.Point(28, 20)
$form.Controls.Add($title)

$what = New-Object System.Windows.Forms.Label
$what.AutoSize = $false
$what.Size = New-Object System.Drawing.Size(560, 44)
$what.Location = New-Object System.Drawing.Point(30, 58)
$what.ForeColor = $dim
$what.Text = "Release folder: " + (Split-Path -Leaf $here)
$form.Controls.Add($what)

$net = New-Object System.Windows.Forms.Label
$net.AutoSize = $false
$net.Size = New-Object System.Drawing.Size(560, 26)
$net.Location = New-Object System.Drawing.Point(30, 100)
$form.Controls.Add($net)
function Refresh-Net {
  if ([System.Net.NetworkInformation.NetworkInterface]::GetIsNetworkAvailable()) {
    $net.ForeColor = $red
    $net.Text = "This computer is online. Turn Wi-Fi off or unplug the cable first."
  } else {
    $net.ForeColor = $dim
    $net.Text = "Offline. Good."
  }
}
Refresh-Net
$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 2000
$timer.Add_Tick({ Refresh-Net })
$timer.Start()

$which = New-Object System.Windows.Forms.Label
$which.Text = "Your key number, as written on your paper:"
$which.AutoSize = $true
$which.Location = New-Object System.Drawing.Point(30, 134)
$form.Controls.Add($which)

$keys = @()
foreach ($n in 1..3) {
  $r = New-Object System.Windows.Forms.RadioButton
  $r.Text = "Key $n"
  $r.AutoSize = $true
  $r.ForeColor = $bone
  $r.Location = New-Object System.Drawing.Point((30 + ($n - 1) * 110), 160)
  $form.Controls.Add($r)
  $keys += $r
}

$ask = New-Object System.Windows.Forms.Label
$ask.Text = "Type your 24 words, with a space between each:"
$ask.AutoSize = $true
$ask.Location = New-Object System.Drawing.Point(30, 198)
$form.Controls.Add($ask)

$words = New-Object System.Windows.Forms.TextBox
$words.Location = New-Object System.Drawing.Point(30, 226)
$words.Size = New-Object System.Drawing.Size(550, 28)
$words.BackColor = $panel
$words.ForeColor = $bone
$words.BorderStyle = "FixedSingle"
$words.UseSystemPasswordChar = $true
$form.Controls.Add($words)

$show = New-Object System.Windows.Forms.CheckBox
$show.Text = "Show the words while I type"
$show.AutoSize = $true
$show.ForeColor = $dim
$show.Location = New-Object System.Drawing.Point(30, 262)
$show.Add_CheckedChanged({ $words.UseSystemPasswordChar = -not $show.Checked })
$form.Controls.Add($show)

$go = New-Object System.Windows.Forms.Button
$go.Text = "SIGN THE RELEASE"
$go.Size = New-Object System.Drawing.Size(220, 44)
$go.Location = New-Object System.Drawing.Point(30, 300)
$go.FlatStyle = "Flat"
$go.FlatAppearance.BorderColor = $red
$go.BackColor = [System.Drawing.Color]::FromArgb(40, 10, 10)
$go.ForeColor = $bone
$form.Controls.Add($go)
$form.AcceptButton = $go

$log = New-Object System.Windows.Forms.TextBox
$log.Multiline = $true
$log.ReadOnly = $true
$log.ScrollBars = "Vertical"
$log.Location = New-Object System.Drawing.Point(30, 360)
$log.Size = New-Object System.Drawing.Size(550, 140)
$log.BackColor = $panel
$log.ForeColor = $dim
$log.BorderStyle = "FixedSingle"
$log.Font = New-Object System.Drawing.Font("Consolas", 9)
$form.Controls.Add($log)

$note = New-Object System.Windows.Forms.Label
$note.AutoSize = $false
$note.Size = New-Object System.Drawing.Size(560, 50)
$note.Location = New-Object System.Drawing.Point(30, 508)
$note.ForeColor = $dim
$note.Text = "Your words stay in this window. Nothing is saved or sent anywhere."
$form.Controls.Add($note)

if (-not (Test-Path $exe)) {
  $what.Text = "release.exe is missing. Right-click the zip, choose Extract All, and open this from the extracted folder."
  $go.Enabled = $false
} else {
  $r = Run-Release "check ." ""
  $log.Text = ($r.Out + $r.Err).Trim() -replace "`n", "`r`n"
  if ($log.Text -match "nothing to sign") {
    $what.Text = "There is nothing to sign here. Right-click the zip, choose Extract All, and open this from the extracted folder."
    $go.Enabled = $false
  }
}

$go.Add_Click({
  $n = 0
  for ($i = 0; $i -lt 3; $i++) { if ($keys[$i].Checked) { $n = $i + 1 } }
  if ($n -eq 0) {
    $note.ForeColor = $red
    $note.Text = "Choose your key number first."
    return
  }
  $text = ($words.Text -replace '\s+', ' ').Trim().ToLower()
  $count = if ($text) { ($text -split ' ').Count } else { 0 }
  if ($count -ne 24) {
    $note.ForeColor = $red
    $note.Text = "That is $count words. It needs all 24, in order, with a space between each."
    return
  }
  $go.Enabled = $false
  $note.ForeColor = $dim
  $note.Text = "Signing. This takes a few seconds."
  $form.Refresh()

  $r = Run-Release "sign-all - $n ." $text
  $words.Text = ""
  $text = $null
  $log.Text = ($r.Out + $r.Err).Trim() -replace "`n", "`r`n"

  if ($r.Code -ne 0) {
    $note.ForeColor = $red
    if ($r.Err -match "not release key") {
      $note.Text = "Those words are not key $n. Check the number you chose and the words on your paper."
    } elseif ($r.Err -match "24 good words") {
      $note.Text = "One of those words is misspelt or out of order. Check them against your paper."
    } else {
      $note.Text = "It did not sign. Tell Dylan what the box above says."
    }
    $go.Enabled = $true
    return
  }

  # Zip the signed folder beside it, ready to send back.
  $parent = Split-Path -Parent $here
  $zip = Join-Path $parent ((Split-Path -Leaf $here) + "-signed-key-$n.zip")
  if (Test-Path $zip) { Remove-Item $zip -Force }
  # Windows PowerShell 5.1's Compress-Archive writes backslashes into the zip's paths, which other
  # systems read as one long file name; ZipFile writes them with forward slashes.
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  [System.IO.Compression.ZipFile]::CreateFromDirectory($here, $zip, 'Optimal', $true)
  $note.ForeColor = $bone
  if ($log.Text -match "Everything in this folder is signed") {
    $note.Text = "Signed by two keys. Turn the internet back on and send Dylan the file now highlighted."
  } else {
    $note.Text = "Signed. Turn the internet back on and send the file now highlighted to the next holder, or to Dylan."
  }
  Start-Process explorer.exe "/select,`"$zip`""
})

[void]$form.ShowDialog()
