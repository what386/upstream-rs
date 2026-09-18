# frozen_string_literal: true

# Future provider API mocks can register handlers here without changing the
# HTTP transport or artifact-serving code.
module LocalArtifactServer
  # Dispatches future provider mock handlers by method and path.
  class ApiRouter
    def initialize
      @routes = {}
    end

    def get(path, &)
      register('GET', path, &)
    end

    def head(path, &)
      register('HEAD', path, &)
    end

    def register(method, path, &handler)
      raise ArgumentError, 'a route handler is required' unless handler

      @routes[[method.upcase, path]] = handler
    end

    def call(request)
      handler = @routes[[request.method, request.path]]
      handler&.call(request)
    end
  end
end
