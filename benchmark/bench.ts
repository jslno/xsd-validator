import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { Bench } from "tinybench"
import { XsdValidator } from "../index.js"

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

const __dirname = dirname(fileURLToPath(import.meta.url));

// 1. Create validators once (outside Bench) — measure validate, not create
const small = await XsdValidator.create(ROOT_XSD, (filename) => {
  switch (filename) {
    case "common/base.xsd":
      return base;
    case "common/types.xsd":
      return types;
  }

  throw new Error(`unexpected schema: ${filename}`);
});
const einfachXml = readFileSync(join(__dirname, "../__test__/assets/XRECHNUNG_Einfach.xml"), "utf-8");
const xrechnung = await XsdValidator.create(readFileSync(join(__dirname, "../__test__/schemas/CrossIndustryInvoice_100pD22B.xsd"), "utf-8"), (filename) => readFileSync(join(__dirname, "../__test__/schemas", filename), "utf-8"));

const bench = new Bench()

bench.add('validate: small valid XML', () => {
  small.validate(`<?xml version="1.0" encoding="UTF-8"?>
<Person xmlns="urn:example">
  <name>Ada</name>
  <age>36</age>
</Person>`)
})

bench.add('validate: small invalid XML', () => {
  small.validate(`<?xml version="1.0" encoding="UTF-8"?>
<Person xmlns="urn:example">
  <name></name>
  <age>0</age>
</Person>`)
})

bench.add('validate: XRechnung Einfach', () => {
  xrechnung.validate(einfachXml)
})

await bench.run()
console.table(bench.table())
