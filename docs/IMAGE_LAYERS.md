# PRO layers and relative brightness / PRO 图层与相对亮度

The 0.1.5rc1 Full viewer uses a common layer-list style for image stacks and
spectrum overlays: visibility, up/down order, opacity, label and display scale.
Select a row to load it and make it the reference. Spectra retain their own native
grids and optional epoch/phase labels. No spectrum subtraction is implemented.

0.1.5rc1 全功能版采用统一图层列表管理多张图像和多条光谱：显隐、上下层序、透明度、标签与
显示比例。点击条目可加载并设为参考。光谱分别保留原采样网格，可填写历元/相位标签；
目前没有多谱相减功能。

The active original is the reference canvas and measurement source, even when its
display is hidden. Auxiliary image textures use at most 1,048,576 display pixels
each and at most eight textures; opacity/order changes reuse them. File data are
shared, not duplicated into scientific resampled images. Mini releases the layer
workspace and its textures. The existing 128 MiB LRU file-cache budget includes
spectrum axes/errors; active main/secondary data can exceed that budget.

当前原始文件作为参考画布和测量源，隐藏显示也不会改变测量来源。每个附加图像纹理最多
1,048,576 个显示像素、最多八个纹理；只改透明度或层序会复用纹理。原始数组共享，显示
预览不生成科研重采样图。极简释放图层工作区与纹理。128 MiB LRU 文件缓存计入光谱
横轴/误差；主图与独立对照图仍可能超过该预算。

Layers share the reference low/high limits, stretch, tone and palette. A layer's
native physical value is mapped as `value * scale + offset` before this shared
display transfer. Nonfinite/BLANK layer pixels are transparent. The main original
keeps its normal-resolution texture; small auxiliary previews can lose fine
features at high zoom. A pixel preview without established registration is
explicitly labeled. Successful WCS mapping is a bounded affine approximation,
not a general distortion-corrected or flux-conserving reprojection.

图层共用参考图的阈值、拉伸、亮度/对比度与色表，先以 `value * scale + offset` 映射。
附加层无效点透明；主图保留正常分辨率，附加预览在高倍率可能损失细节。尚未确认配准的
图层明确标为像素预览。WCS 映射是受限仿射近似，不是完整畸变校正或流量守恒重投影。

## Match brightness from field stars / 按场星匹配亮度

From an image layer choose **Match brightness from field stars**. The selected
external NumPy/SciPy/Astropy environment detects isolated source candidates,
matches them with celestial WCS or explicitly confirmed aligned pixels, measures
background-annulus-subtracted circular-aperture fluxes and clips their log ratios.
It returns the multiplier `reference_flux / target_flux`, selected/rejected counts,
scatter, geometry quality and provenance. A valid current-context result updates
display scale only. Failure, too few sources or a stale source/reference leaves
the existing scale unchanged.
Changing an accepted WCS solution clears cached layer registration and invalidates
in-flight matching replies. Existing display multipliers remain editable.

图像条目中点击**按场星匹配亮度**，调用所选外部 NumPy/SciPy/Astropy 环境，检测孤立候选
源，按天球 WCS 或用户明确确认的已配准像素匹配，以环背景扣除后的圆孔径净流量建立
稳健比例。结果提供 `参考通量 / 目标通量`、选入/排除数量、散布、几何质量与来源记录。
仅将有效且属于当前源/参考/修订号的结果应用到显示比例；失败、少星或过期结果不改比例。
改变已接受的 WCS 坐标解时，清除旧图层配准并使进行中结果失效，已有显示比例仍可手动修改。

This is relative native-count scaling. It does not obtain catalogue magnitudes,
absolute zeropoints, physical flux units or a new exposure correction. Pixel
matching requires an explicit aligned-grid declaration. Unknown/different filters
block automatic application by default; a deliberate override produces an
approximate display comparison. Identical filter names across instruments still
need scientific judgment. Seeing, aperture losses, color terms, variable sources,
unknown saturation/DQ and background structure can bias the multiplier.

这是原始计数的相对匹配，不查询标准星等、不推断绝对零点/物理流量，也不再乘一次曝光
时间。像素模式必须明确确认网格已配准。默认拒绝未知/不同滤镜；用户明确越过时只作近似
显示对照。不同仪器的同名滤镜也未必通带相同；视宁度、孔径损失、色项、变源、未知饱和/
DQ 与背景结构都可能影响比例。

The apertures use pixel-center inclusion and annular median backgrounds, without
PSF fitting or aperture correction. Annular scatter provides a screening SNR,
not formal source Poisson/gain errors; `factor_uncertainty` remains null. This
does not subtract inter-image backgrounds. WCS geometry exceeding 0.5-pixel
affine residual is labeled approximate and is not applied as verified alignment.
The original FITS, errors, header and aperture measurements remain unchanged.

孔径按像素中心入选，背景为环区中值，没有 PSF 拟合或孔径修正；环区散布仅作 SNR
筛选，不是正式 Poisson/gain 误差，`factor_uncertainty` 为 null。不会自动扣掉两图背景差。
WCS 仿射拟合残差超过 0.5 像素时只标为近似，不作为已验证配准应用。
原 FITS、误差、头信息与孔径测量保持不变。

The external operation is `image_flux_match`; the source viewer command
`image.match_flux` supplies a target layer `index` and optional `params`.
`layer.configure` accepts a zero-based `index` and bounded `settings`.
These actions run only in PRO; no scientific package is automatically installed.

外部操作名为 `image_flux_match`；查看器命令 `image.match_flux` 指定目标图层 `index` 与
可选 `params`。`layer.configure` 使用零起始 `index` 与受限 `settings`。均仅用于 PRO，
不会自动安装科学包。
