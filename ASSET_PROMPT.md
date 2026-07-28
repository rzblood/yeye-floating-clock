# 椰椰角色素材生成记录

模式：Codex 内置 ImageGen（身份保持 + 背景提取），生成绿色幕布版本后使用本地色键工具转换为透明 PNG。

最终提示词：

> Use case: identity-preserve
>
> Asset type: Windows transparent desktop-pet sprite, front-facing idle pose
>
> Primary request: Recreate the character from the supplied third reference image as a clean full-body desktop-pet cutout. Preserve the exact same adult woman's facial identity and facial proportions, pale pink bob haircut, grey-green eyes, subtle makeup, calm smile, and white rose flower crown. Preserve the plush purple sweet-potato costume, its rounded tapered body, tiny dark sweet-potato skin marks, the visible orange cut top, and two raised plush arms. Naturally complete the lower body that is cropped in the reference, ending in a softly rounded base suitable for standing on a desktop window edge.
>
> Input image: the most recent image is the identity and costume reference; it is the sole source of facial identity.
>
> Scene/backdrop: perfectly flat solid #00ff00 chroma-key background for background removal, one uniform color with no shadows, gradients, texture, floor plane, reflections, or lighting variation.
>
> Style/medium: polished semi-photorealistic 3D plush mascot, slightly simplified for a small desktop sprite while retaining the face faithfully.
>
> Composition/framing: complete character centered, straight-on, entire silhouette visible with generous padding, symmetric neutral idle pose.
>
> Lighting/mood: soft even studio lighting on the subject only, friendly and gentle.
>
> Constraints: Face must remain consistent with the supplied reference. Keep all key costume details. Crisp separated silhouette. Do not use #00ff00 anywhere in the character. No cast shadow, contact shadow, reflection, text, watermark, scenery, props, or extra limbs.
> Avoid: identity drift, anime face, childlike face, cropped body, background objects, green spill, exaggerated expression.

生成后的角色素材进一步进行了确定性的本地处理：仅压缩脸部以下的身体长度、保留脸部原始比例，并拆分左右手臂图层用于摆动动画。
