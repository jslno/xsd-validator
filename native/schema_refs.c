/**
 * Collect xs:include / xs:import / xs:redefine / xs:override schemaLocation
 * values from an XSD document. Implemented in C so we use libxml2's real
 * xmlNode layout instead of a hand-maintained Rust mirror.
 */

#include <string.h>
#include <libxml/parser.h>
#include <libxml/tree.h>
#include <libxml/xmlmemory.h>

typedef void (*xsd_schema_ref_callback)(void *ctx, const char *location);

/*
 * Release a block allocated by libxml2 through whichever allocator is
 * currently installed. `xmlFree` may be a plain variable or a thread-local
 * macro depending on build flags, so resolving it from C is the only correct
 * option. Never use xmlMemFree here: it is the debug allocator's free and
 * rejects blocks that lack a MEMHDR header.
 */
void
xsd_xml_free(void *ptr)
{
  if (ptr != NULL) {
    xmlFree(ptr);
  }
}

static int
is_xsd_dependency_element(xmlNode *node)
{
  const xmlChar *name;
  const xmlChar *href;

  if (node == NULL || node->type != XML_ELEMENT_NODE || node->name == NULL) {
    return 0;
  }

  name = node->name;
  if (xmlStrEqual(name, BAD_CAST "include") == 0 &&
      xmlStrEqual(name, BAD_CAST "import") == 0 &&
      xmlStrEqual(name, BAD_CAST "redefine") == 0 &&
      xmlStrEqual(name, BAD_CAST "override") == 0) {
    return 0;
  }

  if (node->ns == NULL || node->ns->href == NULL) {
    return 1;
  }

  href = node->ns->href;
  return xmlStrEqual(href, BAD_CAST "http://www.w3.org/2001/XMLSchema");
}

static void
walk(xmlNode *node, xsd_schema_ref_callback cb, void *ctx)
{
  xmlNode *cur;

  for (cur = node; cur != NULL; cur = cur->next) {
    if (is_xsd_dependency_element(cur)) {
      xmlChar *location = xmlGetProp(cur, BAD_CAST "schemaLocation");
      if (location != NULL) {
        if (location[0] != '\0') {
          cb(ctx, (const char *)location);
        }
        xmlFree(location);
      }
    }
    if (cur->children != NULL) {
      walk(cur->children, cb, ctx);
    }
  }
}

int
xsd_collect_schema_locations(const char *xml, int len, const char *url,
                             xsd_schema_ref_callback cb, void *ctx)
{
  xmlDoc *doc;
  xmlNode *root;

  if (xml == NULL || len < 0 || cb == NULL) {
    return -1;
  }

  doc = xmlReadMemory(xml, len, url, NULL, XML_PARSE_NONET | XML_PARSE_COMPACT);
  if (doc == NULL) {
    return -1;
  }

  root = xmlDocGetRootElement(doc);
  if (root != NULL) {
    walk(root, cb, ctx);
  }

  xmlFreeDoc(doc);
  return 0;
}
