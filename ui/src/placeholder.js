export const PLACEHOLDER =
  'data:image/svg+xml;utf8,' +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 320">' +
      '<rect width="320" height="320" fill="#1b2028"/>' +
      '<path d="M80 240l55-70 40 48 30-36 45 58H80z" fill="#2d3642"/>' +
      '<circle cx="120" cy="115" r="22" fill="#2d3642"/>' +
      '</svg>',
  )

export function onImgError(event) {
  if (event.target.src !== PLACEHOLDER) event.target.src = PLACEHOLDER
}