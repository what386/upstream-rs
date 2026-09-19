# frozen_string_literal: true

require_relative '../http/response'

# Page routes can register handlers here without changing the HTTP transport
# or artifact-serving code.
module LocalArtifactServer
  # Dispatches page handlers by HTTP method and path.
  class PageRouter
    def initialize(pages_root: __dir__)
      @routes = {}
      @pages_root = File.expand_path(pages_root)
    end

    def get(path, &)
      register('GET', path, &)
    end

    def head(path, &)
      register('HEAD', path, &)
    end

    def register(method, path, &handler)
      raise ArgumentError, 'a page handler is required' unless handler

      @routes[[method.upcase, path]] = handler
    end

    def call(request)
      handler = @routes[[request.method, request.path]]
      return handler.call(request) if handler

      static_page(request)
    end

    private

    def static_page(request)
      return unless request.path.start_with?('/pages/')

      relative = request.path.delete_prefix('/pages/')
      return if relative.empty? || relative.include?('..')

      path = File.expand_path(relative, @pages_root)
      return unless path.start_with?("#{@pages_root}/") && File.file?(path)

      Response.new(status: 200, body: File.binread(path), content_type: 'text/html; charset=utf-8')
    end
  end
end
