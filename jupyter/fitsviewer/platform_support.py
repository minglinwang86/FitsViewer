"""Small platform helpers without changing the numeric viewer. / 平台路径与进程辅助。"""
import os
from pathlib import Path
import subprocess


def python_at(prefix):
    prefix=Path(prefix)
    candidates=[prefix/'python.exe',prefix/'Scripts/python.exe'] if os.name=='nt' else [prefix/'bin/python',prefix/'bin/python3']
    return next((p for p in candidates if p.is_file()),candidates[0])


def script_at(prefix,name):
    return Path(prefix)/('Scripts/'+name+'.exe' if os.name=='nt' else 'bin/'+name)


def alive(pid):
    if os.name!='nt':
        try: os.kill(pid,0); return True
        except OSError: return False
    # os.kill(pid, 0) can terminate a Windows process; use a read-only query.
    # Windows 使用只读句柄检查，不能沿用 Unix 的信号探测。
    import ctypes
    from ctypes import wintypes
    kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    kernel.OpenProcess.argtypes=[wintypes.DWORD,wintypes.BOOL,wintypes.DWORD]
    kernel.OpenProcess.restype=wintypes.HANDLE
    kernel.GetExitCodeProcess.argtypes=[wintypes.HANDLE,ctypes.POINTER(wintypes.DWORD)]
    kernel.CloseHandle.argtypes=[wintypes.HANDLE]
    handle=kernel.OpenProcess(0x1000,False,pid)
    if not handle: return False
    try:
        code=wintypes.DWORD()
        return bool(kernel.GetExitCodeProcess(handle,ctypes.byref(code))) and code.value==259
    finally: kernel.CloseHandle(handle)


def shell_command(parts):
    if os.name=='nt': return '& '+subprocess.list2cmdline([str(p) for p in parts])
    import shlex
    return shlex.join([str(p) for p in parts])
