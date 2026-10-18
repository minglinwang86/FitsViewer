// Existing FitsViewer brand geometry; keep in sync with web/icons/fitsviewer.svg.
// 复用已有品牌图形；与 web/icons/fitsviewer.svg 保持一致。
import { LabIcon } from '@jupyterlab/ui-components';

export const fitsviewerIcon = new LabIcon({
  name: 'fitsviewer:brand',
  svgstr: `<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256"><rect width="256" height="256" rx="46" fill="#152438"/><path d="M128 27 150 106 229 128 150 150 128 229 106 150 27 128 106 106Z" fill="#67d9e8"/><circle cx="58" cy="58" r="8" fill="white"/><circle cx="195" cy="191" r="12" fill="white"/></svg>`
});
