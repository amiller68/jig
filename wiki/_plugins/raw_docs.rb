module Jekyll
  class RawDocPage < Page
    def initialize(site, base, doc)
      @site = site
      @base = base
      @dir = File.join("docs", doc.data["slug"])
      @name = "raw.txt"

      self.process(@name)
      self.data = {
        "layout" => nil,
        "title" => doc.data["title"],
      }
      self.content = doc.content
    end

    def output_ext
      ".txt"
    end
  end

  class LlmsFullPage < Page
    def initialize(site, base, docs, nav)
      @site = site
      @base = base
      @dir = ""
      @name = "llms-full.txt"

      self.process(@name)
      self.data = {"layout" => nil}

      docs_by_slug = {}
      docs.each { |d| docs_by_slug[d.data["slug"]] = d }

      parts = ["# #{site.config["title"]}\n\n> #{site.config["description"]}\n"]

      nav.each do |section|
        slugs = []
        section["items"].each do |item|
          if item.is_a?(Hash) && item["group"]
            item["group"]["items"].each { |s| slugs << s }
          else
            slugs << item
          end
        end

        slugs.each do |slug|
          doc = docs_by_slug[slug]
          next unless doc
          parts << "\n---\n\n# #{doc.data["title"]}\n\n#{doc.content}"
        end
      end

      self.content = parts.join("\n")
    end

    def output_ext
      ".txt"
    end
  end

  class RawDocsGenerator < Generator
    safe true
    priority :low

    def generate(site)
      docs = site.collections["docs"]
      return unless docs

      docs.docs.each do |doc|
        site.pages << RawDocPage.new(site, site.source, doc)
      end

      nav = site.data["nav"]
      if nav
        site.pages << LlmsFullPage.new(site, site.source, docs.docs, nav)
      end
    end
  end
end
