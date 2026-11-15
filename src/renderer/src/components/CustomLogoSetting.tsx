import { useRef, useState } from 'react'

import {
  CUSTOM_LOGO_MAX_BYTES,
  CUSTOM_LOGO_MIME_TYPES,
  isAcceptedLogoType,
  parseCustomLogo,
  type CustomLogo
} from '../lib/custom-logo'

const BYTES_PER_KILOBYTE = 1024
const MAX_SIZE_LABEL = `${CUSTOM_LOGO_MAX_BYTES / BYTES_PER_KILOBYTE} KB`

interface CustomLogoSettingProps {
  logo: CustomLogo
  onChange: (logo: string) => void
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result))
    reader.onerror = () => reject(reader.error ?? new Error('Could not read the file'))
    reader.readAsDataURL(file)
  })
}

/** Lets the operator swap the header's NEON logo for their own image. */
export function CustomLogoSetting({ logo, onChange }: CustomLogoSettingProps): React.JSX.Element {
  const inputRef = useRef<HTMLInputElement>(null)
  const [error, setError] = useState<string | null>(null)

  const pickFile = async (file: File): Promise<void> => {
    if (!isAcceptedLogoType(file.type)) {
      setError('Use a PNG, JPEG, WebP, GIF or SVG image.')
      return
    }
    if (file.size > CUSTOM_LOGO_MAX_BYTES) {
      setError(`The image is too large — keep it under ${MAX_SIZE_LABEL}.`)
      return
    }
    try {
      const dataUrl = parseCustomLogo(await readFileAsDataUrl(file))
      if (dataUrl === null) {
        setError('This image could not be used as a logo.')
        return
      }
      setError(null)
      onChange(dataUrl)
    } catch (readError) {
      console.error('Failed to read the logo file:', readError)
      setError('The image could not be read.')
    }
  }

  return (
    <div className="setting-row">
      <span className="setting-text">
        <span className="setting-label">Header logo</span>
        <span className="setting-hint">
          Replaces the NEON logo at the top left with your own image (PNG, JPEG, WebP, GIF or SVG,
          up to {MAX_SIZE_LABEL}). A wide image with a transparent background fits best.
        </span>
        {error !== null && (
          <span className="setting-hint setting-error" role="alert">
            {error}
          </span>
        )}
      </span>
      <span className="custom-logo-actions">
        {logo !== null && <img className="custom-logo-preview" src={logo} alt="Current logo" />}
        <input
          ref={inputRef}
          type="file"
          accept={CUSTOM_LOGO_MIME_TYPES.join(',')}
          hidden
          onChange={(event) => {
            const file = event.target.files?.[0]
            // Cleared so picking the same file again still fires a change.
            event.target.value = ''
            if (file !== undefined) void pickFile(file)
          }}
        />
        <button type="button" className="setting-btn" onClick={() => inputRef.current?.click()}>
          Choose image…
        </button>
      </span>
    </div>
  )
}
