import { instantiate } from '@zugferd/xsd-validator-wasm32-wasip1/workerd'
import wasmModule from '@zugferd/xsd-validator-wasm32-wasip1/wasm.wasm'

// Selected by the `workerd` export condition. The browser loader fetches the
// wasm module through `new URL(..., import.meta.url)`, which workerd rejects
// while the module is starting. This entry takes the precompiled module
// Wrangler emits for the `.wasm` import.
const binding = await instantiate(wasmModule)

export const XsdValidator = binding.XsdValidator
export const shutdownLibxml = binding.shutdownLibxml
export const __napiBindingTarget = binding.__napiBindingTarget
export default binding
