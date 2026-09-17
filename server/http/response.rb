# frozen_string_literal: true

module LocalArtifactServer
  STATUS_TEXT = {
    200 => 'OK',
    400 => 'Bad Request',
    404 => 'Not Found',
    405 => 'Method Not Allowed',
    500 => 'Internal Server Error'
  }.freeze

  Response = Struct.new(:status, :body, :content_type, keyword_init: true) do
    def write(socket, request = nil)
      body = self.body.to_s.b
      headers = [
        "HTTP/1.1 #{status} #{LocalArtifactServer::STATUS_TEXT.fetch(status, 'Unknown')}",
        "Content-Type: #{content_type}",
        "Content-Length: #{body.bytesize}",
        'Connection: close',
        "\r\n"
      ]
      socket.write(headers.join("\r\n"))
      socket.write(body) unless request&.method == 'HEAD'
    end
  end
end
