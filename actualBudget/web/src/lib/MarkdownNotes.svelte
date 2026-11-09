<script module lang="ts">
  import { unified, type Processor } from "unified";
  import remarkParse from "remark-parse";
  import remarkGfm from "remark-gfm";
  import remarkBreaks from "remark-breaks";
  import remarkRehype from "remark-rehype";
  import rehypeSanitize from "rehype-sanitize";
  import rehypeStringify from "rehype-stringify";
  import { visit } from "unist-util-visit";
  import type { Extension } from "mdast-util-from-markdown";
  import type { Root } from "hast";

  // Same blank-line extension as Actual26.10 util/markdown.ts.
  function sequentialNewlines(this: Processor) {
    const extension: Extension = {
      enter: {
        lineEndingBlank(token) {
          this.enter({ type: "break" }, token);
        },
      },
      exit: {
        lineEndingBlank(token) {
          this.exit(token);
        },
      },
    };
    const data = this.data();
    const extensions =
      data.fromMarkdownExtensions ?? (data.fromMarkdownExtensions = []);
    extensions.push(extension);
  }
  function noteAttributes() {
    return (tree: Root) => {
      visit(tree, "element", (node, index, parent) => {
        if (
          node.tagName === "img" &&
          !node.properties.src &&
          parent &&
          index !== undefined
        ) {
          parent.children[index] = {
            type: "text",
            value: String(node.properties.alt ?? ""),
          };
          return;
        }
        if (node.tagName === "a" && !node.properties.href)
          node.tagName = "span";
        if (
          node.tagName === "a" &&
          typeof node.properties.href === "string" &&
          !node.properties.href.startsWith("#")
        ) {
          node.properties.target = "_blank";
          node.properties.rel = ["noopener", "noreferrer"];
        }
        if (node.tagName === "img") {
          node.properties.loading = "lazy";
          node.properties.referrerPolicy = "no-referrer";
        }
      });
    };
  }
  // No raw HTML; sanitize parser output before adding only fixed safe attributes.
  const markdown = unified()
    .use(remarkParse)
    .use(sequentialNewlines)
    .use(remarkGfm)
    .use(remarkBreaks)
    .use(remarkRehype, { allowDangerousHtml: false })
    .use(rehypeSanitize)
    .use(noteAttributes)
    .use(rehypeStringify);
</script>

<script lang="ts">
  let { text }: { text: string } = $props();
  const html = $derived(String(markdown.processSync(text)));
</script>

<div class="markdown-notes">{@html html}</div>

<style>
  .markdown-notes {
    overflow-wrap: anywhere;
  }
  .markdown-notes :global(p) {
    margin: 0;
  }
  .markdown-notes :global(p:not(:first-child)) {
    margin-top: 0.25rem;
  }
  .markdown-notes :global(ul),
  .markdown-notes :global(ol) {
    margin: 0;
    padding-inline-start: 1.5rem;
  }
  .markdown-notes :global(:first-child) {
    margin-top: 0;
  }
  .markdown-notes :global(:last-child) {
    margin-bottom: 0;
  }
  .markdown-notes :global(img) {
    max-width: 100%;
    height: auto;
  }
  .markdown-notes :global(table) {
    border-collapse: collapse;
    max-width: 100%;
  }
  .markdown-notes :global(th),
  .markdown-notes :global(td) {
    border: 1px solid var(--line);
    padding: 0.25rem 0.5rem;
  }
  .markdown-notes :global(input[type="checkbox"]) {
    width: auto;
    min-height: auto;
    margin-inline-end: 0.4rem;
  }
  .markdown-notes :global(pre) {
    white-space: pre-wrap;
  }
  .markdown-notes :global(a) {
    color: var(--link);
    text-underline-offset: 0.18em;
  }
</style>
