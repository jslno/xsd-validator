import test from 'ava'

import { XsdValidator } from '../index.js'

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
</xs:schema>
`

const TYPES_XSD = `<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:example"
           xmlns="urn:example"
           elementFormDefault="qualified">
  <xs:simpleType name="NameType">
    <xs:restriction base="xs:string">
      <xs:minLength value="1"/>
    </xs:restriction>
  </xs:simpleType>
</xs:schema>
`

const schemas: Record<string, string> = {
  'common/types.xsd': TYPES_XSD,
}

async function loadSchema(filename: string): Promise<string> {
  const content = schemas[filename]
  if (!content) {
    throw new Error(`unknown schema: ${filename}`)
  }
  return content
}

test('validates XML against a multi-file XSD', async (t) => {
  const validator = await XsdValidator.create(ROOT_XSD, loadSchema)

  const validXml = `<?xml version="1.0" encoding="UTF-8"?>
<Person xmlns="urn:example">
  <name>Ada</name>
  <age>36</age>
</Person>`

  t.true(validator.validateOrThrow(validXml))

  const invalidXml = `<?xml version="1.0" encoding="UTF-8"?>
<Person xmlns="urn:example">
  <name></name>
  <age>0</age>
</Person>`

  try {
    validator.validateOrThrow(invalidXml)
    t.fail()
  } catch {
    t.pass()
  }
})

test('creates a validator from a single-file XSD without a loader', async (t) => {
  const root = `<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:example"
           xmlns="urn:example"
           elementFormDefault="qualified">
  <xs:element name="Hello" type="xs:string"/>
</xs:schema>`

  const validator = await XsdValidator.create(root)
  t.true(validator.validate(`<?xml version="1.0" encoding="UTF-8"?><Hello xmlns="urn:example">world</Hello>`))
})

test('throws when includes exist but no loader is provided', async (t) => {
  await t.throwsAsync(
    () =>
      XsdValidator.create(
        `<?xml version="1.0"?><xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:include schemaLocation="missing.xsd"/></xs:schema>`,
      ),
    { message: /no loader was provided/i },
  )
})

test('throws useful errors for invalid schemas', async (t) => {
  await t.throwsAsync(
    () =>
      XsdValidator.create(
        `<?xml version="1.0"?><xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:include schemaLocation="missing.xsd"/></xs:schema>`,
        async () => {
          throw new Error('cannot load')
        },
      ),
    { message: /cannot load|schema loader/i },
  )
})

test('loader is called for nested includes and imports during create only', async (t) => {
  const root = `<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:example"
           xmlns="urn:example"
           elementFormDefault="qualified">
  <xs:include schemaLocation="common/types.xsd"/>
  <xs:import namespace="urn:other" schemaLocation="other.xsd"/>
  <xs:element name="Person">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="name" type="NameType"/>
        <xs:element name="age" type="xs:positiveInteger"/>
        <xs:element ref="other:Note" xmlns:other="urn:other"/>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>`

  const types = `<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:example"
           xmlns="urn:example"
           elementFormDefault="qualified">
  <xs:include schemaLocation="base.xsd"/>
  <xs:simpleType name="NameType">
    <xs:restriction base="BaseString">
      <xs:minLength value="1"/>
    </xs:restriction>
  </xs:simpleType>
</xs:schema>`

  const base = `<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:example"
           xmlns="urn:example"
           elementFormDefault="qualified">
  <xs:simpleType name="BaseString">
    <xs:restriction base="xs:string"/>
  </xs:simpleType>
</xs:schema>`

  const other = `<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:other"
           xmlns="urn:other"
           elementFormDefault="qualified">
  <xs:element name="Note" type="xs:string"/>
</xs:schema>`

  const files: Record<string, string> = {
    'common/types.xsd': types,
    'common/base.xsd': base,
    'other.xsd': other,
  }

  const requested: string[] = []

  const validator = await XsdValidator.create(root, async (filename) => {
    requested.push(filename)
    const content = files[filename]
    if (!content) {
      throw new Error(`unexpected loader request: ${filename}`)
    }
    return content
  })

  t.deepEqual(requested.sort(), ['common/base.xsd', 'common/types.xsd', 'other.xsd'])

  // Loader must not be invoked again during validation.
  const requestedAfterCreate = requested.length
  t.true(
    validator.validate(`<?xml version="1.0" encoding="UTF-8"?>
<Person xmlns="urn:example" xmlns:other="urn:other">
  <name>Ada</name>
  <age>36</age>
  <other:Note>hi</other:Note>
</Person>`),
  )
  t.is(requested.length, requestedAfterCreate)
})

test('creates multiple validators concurrently', async (t) => {
  const makeRoot = (ns: string) => `<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="${ns}"
           xmlns="${ns}"
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

  const makeTypes = (ns: string) => `<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="${ns}"
           xmlns="${ns}"
           elementFormDefault="qualified">
  <xs:simpleType name="NameType">
    <xs:restriction base="xs:string">
      <xs:minLength value="1"/>
    </xs:restriction>
  </xs:simpleType>
</xs:schema>`

  const createOne = async (ns: string) => {
    const types = makeTypes(ns)
    const validator = await XsdValidator.create(makeRoot(ns), async (filename) => {
      if (filename === 'common/types.xsd') {
        // Yield so concurrent creates overlap during schema resolution.
        await new Promise((resolve) => setTimeout(resolve, 5))
        return types
      }
      throw new Error(`unexpected schema: ${filename}`)
    })

    t.true(
      validator.validate(`<?xml version="1.0" encoding="UTF-8"?>
<Person xmlns="${ns}">
  <name>Ada</name>
  <age>36</age>
</Person>`),
    )
    return validator
  }

  const validators = await Promise.all([
    createOne('urn:example:a'),
    createOne('urn:example:b'),
    createOne('urn:example:c'),
    createOne('urn:example:d'),
  ])

  t.is(validators.length, 4)
})
