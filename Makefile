.PHONY: wiki wiki-install wiki-build

# Preview wiki locally at http://localhost:4000
wiki:
	cd wiki && bundle install && bundle exec jekyll serve --livereload

# Build wiki without serving
wiki-build:
	cd wiki && bundle exec jekyll build
