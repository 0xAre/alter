# ALTER Demo — VHS Tape Script
# Generate GIF: vhs demo.tape
# Install VHS: https://github.com/charmbracelet/vhs

Output demo.gif

Set Shell "powershell"
Set Width 120
Set Height 120
Set FontSize 14
Set Theme "TokyoNight"
Set Framerate 24
Set PlaybackSpeed 1.0

# ─── Scene 1: Buat vault demo baru ───────────────────────
Type "alter --offline --vault C:/alter_demo.key"
Enter
Sleep 1.5s

# Layar Create muncul — isi passphrase ALTER
Type "demo1234"
Enter
Sleep 0.5s
Type "demo1234"
Enter
Sleep 0.6s

# Passphrase PM (decoy front)
Type "decoy1234"
Enter
Sleep 0.5s
Type "decoy1234"
Enter
Sleep 3s

# ─── Scene 2: Main screen ────────────────────────────────
Sleep 1.5s

# ─── Scene 3: Invite code ────────────────────────────────
Type "i"
Sleep 3s

# Tutup invite code
Type "i"
Sleep 1s

# ─── Scene 4: Form tambah kontak ─────────────────────────
Type "a"
Sleep 1.5s

Escape
Sleep 0.8s

# ─── Scene 5: Keluar bersih ──────────────────────────────
Type "q"
Sleep 0.5s
