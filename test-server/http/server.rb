# frozen_string_literal: true

require 'socket'

require_relative 'request'
require_relative 'response'
require_relative '../artifacts/store'
require_relative '../pages/router'

module LocalArtifactServer
  # Owns socket lifecycle and dispatches requests to artifact/page handlers.
  class HttpServer
    def initialize(host:, port:, artifact_root:, page_router: PageRouter.new, logger: $stdout)
      @host = host
      @port = Integer(port)
      @artifact_store = ArtifactStore.new(artifact_root)
      @page_router = page_router
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

      return @artifact_store.call(request) if request.path.start_with?(ArtifactStore::PREFIX)

      @page_router.call(request) || ArtifactStore::NOT_FOUND
    end

    def log(message)
      @logger.puts(message)
      @logger.flush
    end
  end
end
