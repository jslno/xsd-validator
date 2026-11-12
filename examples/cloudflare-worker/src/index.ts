import { XsdValidator } from '@zugferd/xsd-validator'

const ROOT_XSD = `<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:example"
           xmlns="urn:example"
           elementFormDefault="qualified">
  <xs:include schemaLocation="common/types.xsd"/>
  <xs:element name="Person">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="name" type="NameType"/>
        <xs:element name="age" type="xs:positiveInteger"/>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>`

const SCHEMAS: Record<string, string> = {
  'common/types.xsd': `<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:example"
           xmlns="urn:example"
           elementFormDefault="qualified">
  <xs:simpleType name="NameType">
    <xs:restriction base="xs:string">
      <xs:minLength value="1"/>
    </xs:restriction>
  </xs:simpleType>
</xs:schema>`,
}

// The Workers entry instantiates the wasm module on import. Compile the schema
// once per isolate and reuse it across requests.
let validator: Promise<XsdValidator> | undefined

async function getValidator(): Promise<XsdValidator> {
  validator ??= XsdValidator.create(ROOT_XSD, (filename) => {
    const schema = SCHEMAS[filename]
    if (!schema) {
      throw new Error(`unknown schema: ${filename}`)
    }
    return schema
  })
  return validator
}

export default {
  async fetch(request: Request): Promise<Response> {
    if (request.method !== 'POST') {
      return new Response('POST an XML document to validate it.\n', { status: 405 })
    }

    const xml = await request.text()
    const xsd = await getValidator()

    try {
      xsd.validateOrThrow(xml)
      return Response.json({ valid: true })
    } catch (error) {
      return Response.json({ valid: false, errors: (error as Error).message }, { status: 422 })
    }
  },
}
