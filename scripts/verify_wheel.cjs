// Browser checks against a pip-installed wheel in a fresh environment.
const fs = require('fs');
const path = require('path');
const assert = require('node:assert/strict');
const {execFileSync} = require('child_process');
const {chromium} = require('../qa/node_modules/playwright');
const root = path.resolve(__dirname, '..');
const config = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const server = JSON.parse(fs.readFileSync(config.server_info, 'utf8'));
const fitsviewer = path.join(config.venv, process.platform === 'win32' ? 'Scripts/fitsviewer.exe' : 'bin/fitsviewer');
const shots = path.join(root, 'reports/screenshots');
fs.mkdirSync(shots, {recursive: true});

(async () => {
  const browser = await chromium.launch({headless: true, args: ['--no-sandbox', '--disable-dev-shm-usage', '--use-angle=swiftshader', '--enable-unsafe-swiftshader']});
  const context = await browser.newContext({viewport: {width: 1400, height: 950}});
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    await page.goto(server.url + 'lab?token=' + server.token, {waitUntil: 'domcontentloaded'});
    await page.getByText('science.fits', {exact: true}).first().dblclick({timeout: 60000});
    let frame = await (await page.locator('iframe[data-testid="fitsviewer-frame"]:visible').last().elementHandle()).contentFrame();
    await frame.waitForFunction(() => window.__fitsviewerState?.loaded, {timeout: 60000});
    assert.equal((await frame.evaluate(() => window.__fitsviewerState)).width, 512);
    const canvas = frame.locator('canvas');
    const box = await canvas.boundingBox();
    await canvas.click({position: {x: 500, y: 400}});
    await page.keyboard.press('2');
    await frame.waitForFunction(() => window.__fitsviewerState.stretch === 'Log');
    await page.mouse.move(box.x + 500, box.y + 400);
    await page.mouse.down({button: 'right'});
    await page.mouse.move(box.x + 570, box.y + 340, {steps: 12});
    await page.mouse.up({button: 'right'});
    await frame.waitForFunction(() => window.__fitsviewerState.bias < 0.5 && window.__fitsviewerState.contrast > 1);
    await page.keyboard.press('R');
    await frame.waitForFunction(() => window.__fitsviewerState.bias === 0.5 && window.__fitsviewerState.contrast === 1);
    await page.screenshot({path: path.join(shots, 'pip_default_fits_viewer.png')});

    execFileSync(fitsviewer, [path.join(config.data_root, 'second.fits'), '--no-browser'], {env: config.environment});
    const second = page.locator('iframe[data-testid="fitsviewer-frame"][src*="second.fits"]:visible');
    await second.waitFor({timeout: 30000});
    frame = await (await second.elementHandle()).contentFrame();
    await frame.waitForFunction(() => window.__fitsviewerState?.loaded);

    execFileSync(fitsviewer, ['defaults', 'set', '--jupyter', 'no'], {env: config.environment});
    const preferences = await page.request.get(server.url + 'fitsviewer-settings');
    assert.equal((await preferences.json()).default_viewer, false);
    const alternate = await context.newPage();
    await alternate.goto(server.url + 'lab/workspaces/default-off', {waitUntil: 'domcontentloaded'});
    await alternate.waitForFunction(() => window.jupyterapp?.docRegistry.getWidgetFactory('FitsViewer'));
    assert.notEqual(await alternate.evaluate(() => window.jupyterapp.docRegistry.defaultWidgetFactory('science.fits').name), 'FitsViewer');
    assert.equal(await alternate.evaluate(() => window.jupyterapp.docRegistry.getWidgetFactory('FitsViewer').name), 'FitsViewer');
    await alternate.close();
    execFileSync(fitsviewer, ['defaults', 'set', '--jupyter', 'yes'], {env: config.environment});

    const notebook = await context.newPage();
    await notebook.goto(server.url + 'lab/workspaces/wheel-notebook/tree/demo.ipynb', {waitUntil: 'domcontentloaded'});
    const editor = notebook.locator('.jp-CodeCell .cm-content').first();
    await editor.waitFor({timeout: 60000});
    // Wait for the first kernel connection before sending the execution shortcut.
    for (let attempt = 0; attempt < 120; attempt++) {
      const reply = await notebook.request.get(server.url + 'api/sessions?token=' + server.token);
      const sessions = await reply.json();
      if (sessions.some(s => s.path === 'demo.ipynb' && s.kernel.execution_state === 'idle')) break;
      if (attempt === 119) throw new Error('Notebook kernel did not become idle');
      await notebook.waitForTimeout(500);
    }
    await editor.click();
    await notebook.keyboard.press('Shift+Enter');
    const embedded = notebook.locator('.jp-OutputArea iframe');
    await embedded.waitFor({timeout: 60000});
    assert.ok((await embedded.getAttribute('src')).startsWith('/wheel-test/fitsviewer/'));
    const embeddedFrame = await (await embedded.elementHandle()).contentFrame();
    await embeddedFrame.waitForFunction(() => window.__fitsviewerState?.loaded, {timeout: 60000});
    await notebook.screenshot({path: path.join(shots, 'pip_notebook_embedded.png')});
    assert.deepEqual(errors, []);
    console.log(JSON.stringify({passed: true, default_double_click: true, stretch: true, right_drag: true,
      cli_opens_in_jupyter: true, notebook_embedded: true, jupyter_url_prefix: true, default_toggle: true, open_with_preserved: true, javascript_errors: errors}));
  } catch (error) {
    await page.screenshot({path: path.join(shots, 'pip_failure.png')}).catch(() => {});
    console.error(error.stack);
    throw error;
  } finally {
    await browser.close();
  }
})();
