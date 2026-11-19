import type { CustomValue, OscMessageConfig, Trigger } from '../model'
import {
  BPM_DECIMAL_OPTIONS,
  CUSTOM_VALUE_OPTIONS,
  TEMPLATE_VARIABLES,
  TRIGGER_OPTIONS,
  createCustomValue,
  messageControls,
  withCustomFields,
  type CustomValueType
} from '../lib/tempo-messages'
import { TempoDraftNumberInput } from './TempoDraftNumberInput'

interface TempoValueParametersProps {
  value: CustomValue
  onChange: (value: CustomValue) => void
}

/** The fields a custom value needs beyond its type: a range, or a template. */
function TempoValueParameters({
  value,
  onChange
}: TempoValueParametersProps): React.JSX.Element | null {
  if (value.type === 'floatNormalized') {
    return (
      <>
        <TempoDraftNumberInput
          className="tempo-range"
          label="Minimum, sent as 0"
          value={value.min}
          onCommit={(min) => onChange({ ...value, min })}
        />
        <TempoDraftNumberInput
          className="tempo-range"
          label="Maximum, sent as 1"
          value={value.max}
          onCommit={(max) => onChange({ ...value, max })}
        />
      </>
    )
  }
  if (value.type === 'template') {
    return (
      <input
        type="text"
        className="setting-text-input tempo-template"
        aria-label="Text template"
        title={`Variables: ${TEMPLATE_VARIABLES.join(' ')}`}
        placeholder={TEMPLATE_VARIABLES.join(' ')}
        value={value.template}
        onChange={(event) => onChange({ type: 'template', template: event.target.value })}
      />
    )
  }
  return null
}

interface TempoMessageFormatProps {
  message: OscMessageConfig
  onChange: (message: OscMessageConfig) => void
}

/** Trigger, argument and BPM decimals of a custom message. */
export function TempoMessageFormat({
  message,
  onChange
}: TempoMessageFormatProps): React.JSX.Element | null {
  const { kind } = message
  if (kind.type !== 'custom') return null
  const controls = messageControls(message)

  return (
    <>
      <select
        className="setting-text-input tempo-select"
        aria-label="Trigger"
        value={kind.trigger}
        onChange={(event) =>
          onChange(withCustomFields(message, { trigger: event.target.value as Trigger }))
        }
      >
        {TRIGGER_OPTIONS.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <select
        className="setting-text-input tempo-select"
        aria-label="Argument"
        value={kind.value.type}
        onChange={(event) =>
          onChange(
            withCustomFields(message, {
              value: createCustomValue(event.target.value as CustomValueType)
            })
          )
        }
      >
        {CUSTOM_VALUE_OPTIONS.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <TempoValueParameters
        value={kind.value}
        onChange={(value) => onChange(withCustomFields(message, { value }))}
      />
      {controls.bpmDecimals && (
        <select
          className="setting-text-input tempo-select"
          aria-label="BPM decimals"
          value={kind.bpmDecimals}
          onChange={(event) =>
            onChange(withCustomFields(message, { bpmDecimals: Number(event.target.value) }))
          }
        >
          {BPM_DECIMAL_OPTIONS.map((decimals) => (
            <option key={decimals} value={decimals}>
              {decimals} decimal{decimals === 1 ? '' : 's'}
            </option>
          ))}
        </select>
      )}
    </>
  )
}
