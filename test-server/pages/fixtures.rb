# frozen_string_literal: true

require 'json'

module LocalArtifactServer
  # Declarative responses for CLI tests, reloaded so tests can publish updates.
  class FixtureRouter
    def initialize(root)
      @root = File.realpath(root)
      @manifest = File.join(@root, '.routes.json')
    end

    def call(request)
      return unless File.file?(@manifest)

      routes = JSON.parse(File.read(@manifest))
      route = routes.find do |item|
        item.fetch('method', request.method) == request.method &&
          item.fetch('path') == request.path &&
          (!item.key?('query') || item['query'] == URI.decode_www_form(request.query.to_s).to_h)
      end
      return unless route

      body = route.fetch('body', '')
      if route.key?('file')
        path = File.realpath(File.join(@root, route['file']))
        raise 'fixture file escapes artifact root' unless path.start_with?("#{@root}#{File::SEPARATOR}")

        body = File.binread(path)
      end
      headers = route.fetch('headers', {}).dup
      status = route.fetch('status', 200)
      if route['ranges']
        headers['Accept-Ranges'] = 'bytes'
        range = request.headers['range']
        if range && route['ranges'] == 'honor'
          match = /\Abytes=(\d+)-(\d+)\z/.match(range)
          first, last = match&.captures&.map(&:to_i)
          return Response.new(status: 416, body: '') unless first && first <= last && last < body.bytesize

          headers['Content-Range'] = "bytes #{first}-#{last}/#{body.bytesize}"
          body = body.byteslice(first..last)
          status = 206
        end
      end
      Response.new(status: status, body: body, headers: headers,
                   content_type: route.fetch('content_type', 'application/octet-stream'),
                   omit_length: route.fetch('omit_length', false))
    end
  end
end
