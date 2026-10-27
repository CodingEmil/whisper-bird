# Lädt PATH und Build-Variablen frisch aus der Registry (nach winget-Installationen nötig).
# Verwendung:  . .\scripts\env.ps1
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
$vk = [Environment]::GetEnvironmentVariable('VULKAN_SDK', 'Machine')
if ($vk) { $env:VULKAN_SDK = $vk }
if (Test-Path 'C:\Program Files\LLVM\bin\libclang.dll') { $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin' }
