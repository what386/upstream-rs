# frozen_string_literal: true

module LocalArtifactServer
  # Safely resolves and serves files below the configured artifact directory.
  class ArtifactStore
    PREFIX = '/artifacts/'
    NOT_FOUND = Response.new(status: 404, body: "not found\n", content_type: 'text/plain; charset=utf-8').freeze

    MIME_TYPES = {
      '.7z' => 'application/x-7z-compressed',
      '.appimage' => 'application/octet-stream',
      '.bz2' => 'application/x-bzip2',
      '.deb' => 'application/vnd.debian.binary-package',
      '.gz' => 'application/gzip',
      '.html' => 'text/html; charset=utf-8',
      '.json' => 'application/json; charset=utf-8',
      '.rpm' => 'application/x-rpm',
      '.tar' => 'application/x-tar',
      '.xz' => 'application/x-xz',
      '.zip' => 'application/zip',
      '.zst' => 'application/zstd'
    }.freeze

    def initialize(root)
      @root = File.realpath(root)
      @request_marker = ENV.fetch('SERVER_REQUEST_MARKER', nil)
      @throttle_pattern = ENV.fetch('SERVER_THROTTLE_PATTERN', nil)
      @throttle_delay = Float(ENV.fetch('SERVER_THROTTLE_DELAY', '0'))
    end

    def call(request)
      return NOT_FOUND unless request.path.start_with?(PREFIX)

      candidate = resolve(request.path.delete_prefix(PREFIX))
      candidate ? response_for(candidate) : NOT_FOUND
    rescue Errno::ENOENT, Errno::ENOTDIR
      NOT_FOUND
    end

    private

    def resolve(relative_path)
      candidate = File.realpath(File.join(@root, relative_path))
      return unless candidate == @root || candidate.start_with?("#{@root}#{File::SEPARATOR}")

      candidate if File.file?(candidate)
    end

    def response_for(path)
      if @throttle_pattern && File.basename(path).include?(@throttle_pattern)
        File.write(@request_marker, "requested\n") if @request_marker
        return throttled_response(path)
      end

      Response.new(
        status: 200,
        body: File.binread(path),
        content_type: MIME_TYPES.fetch(File.extname(path).downcase, 'application/octet-stream')
      )
    end

    def throttled_response(path)
      Response.new(
        status: 200,
        body: nil,
        content_length: File.size(path),
        content_type: MIME_TYPES.fetch(File.extname(path).downcase, 'application/octet-stream'),
        stream_writer: stream_writer_for(path)
      )
    end

    def stream_writer_for(path)
      lambda do |socket|
        File.open(path, 'rb') do |file|
          while (chunk = file.read(16 * 1024))
            socket.write(chunk)
            socket.flush
            sleep(@throttle_delay) if @throttle_delay.positive?
          end
        end
      end
    end
  end
end
