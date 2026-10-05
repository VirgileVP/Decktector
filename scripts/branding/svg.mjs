/** SVG document helpers and PNG rasterization. */
import { Resvg } from '@resvg/resvg-js'

/** Standalone SVG document; the viewBox defaults to the full width × height. */
export const svgDocument = (width, height, body, viewBox = `0 0 ${width} ${height}`) =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="${viewBox}">${body}</svg>`

/** Rasterizes an SVG to a PNG buffer scaled to the given pixel width. */
export const renderPng = (svg, width) =>
  new Resvg(svg, { fitTo: { mode: 'width', value: width } }).render().asPng()

/** Bounding box of the non-transparent pixels of an SVG rendered at 1:1. */
export function inkBounds(svg) {
  const { width, height, pixels } = new Resvg(svg).render()
  let [minX, minY, maxX, maxY] = [width, height, -1, -1]
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (pixels[(y * width + x) * 4 + 3] === 0) continue
      minX = Math.min(minX, x)
      maxX = Math.max(maxX, x)
      minY = Math.min(minY, y)
      maxY = Math.max(maxY, y)
    }
  }
  if (maxX < 0) throw new Error('inkBounds: the SVG renders no visible pixel')
  return { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 }
}
