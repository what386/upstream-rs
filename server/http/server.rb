# frozen_string_literal: true

require 'socket'

require_relative '../apis/router'
require_relative 'request'
require_relative 'response'
require_relative '../artifacts/store'

module LocalArtifactServer
  # Owns socket lifecycle and dispatches requests to the artifact/API handlers.
  class HttpServer
    def initialize(host:, port:, artifact_root:, api_router: ApiRouter.new, logger: $stdout)
      @host = host
      @port = Integer(port)
      @artifact_store = ArtifactStore.new(artifact_root)
      @api_router = api_router
      @logger = logger
      @server = TCPServer.new(@host, @port)
      @stopping = false
    end

    attr_reader :server

    def start
      log("listening on http://#{@host}:#{@server.addr[1]}")
      accept_connections
    end

    def accept_connections
      until @stopping
        socket = accept_socket
        break unless socket

        Thread.new(socket) { |client| serve(client) }
      end
    end

    def accept_socket
      @server.accept
    rescue IOError, Errno::EBADF
      nil
    end

    def serve(client)
      handle(client)
    ensure
      client.close
    end

    def stop
      return if @stopping

      @stopping = true
      @server.close unless @server.closed?
    end

    private

    def handle(socket)
      request = Request.parse(socket)
      response = dispatch(request)
      response.write(socket, request)
      log("#{request.method} #{request.path} #{response.status}")
    rescue RequestError => e
      Response.new(status: e.status, body: e.message, content_type: 'text/plain; charset=utf-8').write(socket)
    rescue StandardError => e
      log("request failed: #{e.class}: #{e.message}")
      Response.new(status: 500, body: "internal server error\n",
                   content_type: 'text/plain; charset=utf-8').write(socket)
    end

    def dispatch(request)
      unless %w[GET HEAD].include?(request.method)
        return Response.new(status: 405, body: "method not allowed\n", content_type: 'text/plain; charset=utf-8')
      end

      if request.path.start_with?('/api/') || request.path == '/api'
        return @api_router.call(request) || ArtifactStore::NOT_FOUND
      end

      @artifact_store.call(request)
    end

    def log(message)
      @logger.puts(message)
      @logger.flush
    end
  end
end
