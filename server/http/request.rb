# frozen_string_literal: true

require 'uri'

module LocalArtifactServer
  # Signals malformed HTTP requests with a response status.
  class RequestError < StandardError
    attr_reader :status

    def initialize(status, message)
      @status = status
      super(message)
    end
  end

  # Reads and validates the request headers from a client socket.
  class HeaderParser
    MAX_HEADERS = 64 * 1024

    def initialize(socket)
      @socket = socket
    end

    def read
      headers = {}
      header_bytes = 0
      loop do
        header = @socket.gets(MAX_HEADERS)
        break if header == "\r\n"

        header_bytes += header&.bytesize || 0
        name, value = parse_header(header, header_bytes)
        headers[name.downcase] = value.strip
      end
      headers
    end

    private

    def parse_header(header, header_bytes)
      raise RequestError.new(400, "bad request\n") unless header&.end_with?("\r\n")
      raise RequestError.new(400, "headers too large\n") if header_bytes > MAX_HEADERS

      name, value = header.strip.split(':', 2)
      raise RequestError.new(400, "bad header\n") unless name && value

      [name, value]
    end
  end

  # Represents the small HTTP request surface used by the local server.
  class Request
    MAX_REQUEST_LINE = 8 * 1024

    attr_reader :method, :path, :query, :headers

    def self.parse(socket)
      method, target = parse_request_line(socket.gets(MAX_REQUEST_LINE))
      headers = HeaderParser.new(socket).read
      path, query = parse_target(target)

      new(method: method.upcase, path: path, query: query, headers: headers)
    rescue URI::InvalidURIError
      raise RequestError.new(400, "bad path\n")
    end

    def initialize(method:, path:, query:, headers:)
      @method = method
      @path = path
      @query = query
      @headers = headers
    end

    def self.parse_request_line(line)
      raise RequestError.new(400, "bad request\n") unless line&.end_with?("\r\n")

      method, target, version = line.strip.split(' ', 3)
      valid = method && target && version&.start_with?('HTTP/')
      raise RequestError.new(400, "bad request\n") unless valid

      [method, target]
    end
    private_class_method :parse_request_line

    def self.parse_target(target)
      raw_path, query = target.split('?', 2)
      path = URI::DEFAULT_PARSER.unescape(raw_path)
      valid = !path.empty? && !path.include?("\0") && path.start_with?('/')
      raise RequestError.new(400, "bad path\n") unless valid

      [path, query]
    end
    private_class_method :parse_target
  end
end
