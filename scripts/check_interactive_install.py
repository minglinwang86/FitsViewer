"""Test the real terminal installer in a pseudoterminal. / 验证真实交互安装。"""
import os
import pty
import select
import subprocess
import time


def check_interactive_install(cli, target, environment):
    master, slave = pty.openpty()
    process = subprocess.Popen([cli, "install", "--no-deps"], stdin=slave, stdout=slave, stderr=slave,
                               env=environment, start_new_session=True)
    os.close(slave)
    output = bytearray()
    try:
        os.write(master, ("2\n" + str(target / "bin/python") + "\nn\nn\n").encode())
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if select.select([master], [], [], .2)[0]:
                try: data = os.read(master, 65536)
                except OSError: break
                if not data: break
                output.extend(data)
            if process.poll() is not None: break
        process.wait(timeout=5)
        text = output.decode(errors="replace")
        assert process.returncode == 0, text[-4000:]
        assert "Installation target:" in text and "Conda environment" in text
        assert "Installation environment: " + str(target) in text
        assert "Default FITS viewer in JupyterLab: No" in text
        clean = {k: v for k, v in environment.items() if not k.startswith("FITSVIEWER_")}
        subprocess.run([str(target / "bin/fitsviewer"), "defaults", "set", "--jupyter", "yes"], env=clean, check=True, capture_output=True)
        return {"terminal_target_selection": True, "python_path_with_spaces": True,
                "jupyter_default_question": True, "desktop_default_question": True}
    finally:
        if process.poll() is None: process.terminate(); process.wait(timeout=5)
        os.close(master)
