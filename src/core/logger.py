"""Logging configuration and service."""
import json
import logging
import logging.config
from typing import Any, Dict, Optional

from .config import Settings


class BaseFormatter(logging.Formatter):
    """Base formatter with common functionality."""

    def __init__(self, settings_instance: Settings) -> None:
        """Initialize formatter.

        Args:
            settings_instance: Settings instance for configuration
        """
        super().__init__()
        self.settings = settings_instance

    # Стандартные атрибуты LogRecord, которые нужно исключить
    STANDARD_LOG_RECORD_ATTRIBUTES = {
        "name",
        "msg",
        "args",
        "levelname",
        "levelno",
        "pathname",
        "filename",
        "module",
        "exc_info",
        "exc_text",
        "stack_info",
        "lineno",
        "funcName",
        "created",
        "msecs",
        "relativeCreated",
        "thread",
        "threadName",
        "processName",
        "process",
        "message",
        "asctime",
    }

    def get_extra_fields(self, record: logging.LogRecord) -> Dict[str, Any]:
        """Get extra fields from record.

        Args:
            record: Log record to process

        Returns:
            Dictionary with extra fields
        """
        extra = {}

        # Собираем все нестандартные атрибуты из LogRecord
        for key, value in record.__dict__.items():
            if key not in self.STANDARD_LOG_RECORD_ATTRIBUTES:
                extra[key] = value

        # Добавляем дополнительные поля из настроек
        for field in self.settings.LOG_EXTRA_FIELDS:
            if hasattr(record, field):
                extra[field] = getattr(record, field)

        return extra


class JsonFormatter(BaseFormatter):
    """JSON formatter for structured logging."""

    def format(self, record: logging.LogRecord) -> str:
        """Format log record as JSON.

        Args:
            record: Log record to format

        Returns:
            JSON string
        """
        log_data: dict[str, str | int | dict[str, Any] | Any] = {
            "time": self.formatTime(record),
            "level": record.levelname,
            "name": record.name,
            "message": record.getMessage(),
        }

        # Добавляем extra поля в корень JSON
        extra = self.get_extra_fields(record)
        if extra:
            # Обрабатываем несериализуемые объекты
            sanitized_extra = {}
            for key, value in extra.items():
                try:
                    # Проверяем, можно ли сериализовать значение
                    json.dumps({key: value})
                    sanitized_extra[key] = value
                except (TypeError, OverflowError):
                    # Если нельзя сериализовать, преобразуем в строку
                    sanitized_extra[key] = f"<non-serializable: {type(value).__name__}>"
            log_data.update(sanitized_extra)

        # Добавляем информацию об исключении
        if record.exc_info:
            exception_text = self.formatException(record.exc_info)
            if exception_text:
                log_data["exception"] = str(exception_text)

        try:
            return json.dumps(log_data)
        except (TypeError, OverflowError) as e:
            # Если все еще не можем сериализовать, возвращаем упрощенное сообщение
            return json.dumps(
                {
                    "time": self.formatTime(record),
                    "level": record.levelname,
                    "name": record.name,
                    "message": record.getMessage(),
                    "error": f"Failed to serialize log: {str(e)}",
                }
            )


class TextFormatter(BaseFormatter):
    """Text formatter for human-readable logging."""

    def format(self, record: logging.LogRecord) -> str:
        """Format log record as text.

        Args:
            record: Log record to format

        Returns:
            Formatted string
        """
        # Базовое сообщение
        msg = (
            f"{self.formatTime(record)} - {record.levelname} - "
            f"{record.name} - {record.getMessage()}"
        )

        # Добавляем extra поля
        extra = self.get_extra_fields(record)
        if extra:
            msg += f" - extra={extra}"

        # Добавляем информацию об исключении
        if record.exc_info:
            msg += f"\n{self.formatException(record.exc_info)}"

        return msg


class StructuredFormatter(BaseFormatter):
    """Structured formatter for key-value pairs."""

    def format(self, record: logging.LogRecord) -> str:
        """Format log record as key-value pairs.

        Args:
            record: Log record to format

        Returns:
            Formatted string
        """
        parts = [
            f"time={self.formatTime(record)}",
            f"level={record.levelname}",
            f"name={record.name}",
            f"message={record.getMessage()}",
        ]

        # Добавляем extra поля
        extra = self.get_extra_fields(record)
        if extra:
            for key, value in extra.items():
                parts.append(f"{key}={value}")

        # Добавляем информацию об исключении
        if record.exc_info:
            parts.append(f"exception={self.formatException(record.exc_info)}")

        return " ".join(parts)


class LoggerService:
    """Service for configuring and providing loggers."""

    def __init__(
        self,
        settings_instance: Settings,
        config: Optional[Dict[str, Any]] = None,
    ) -> None:
        """Initialize logging configuration.

        Args:
            settings_instance: Settings instance to use
            config: Optional logging configuration dictionary
        """
        self.settings = settings_instance
        self.formatters = {
            "json": JsonFormatter(settings_instance),
            "text": TextFormatter(settings_instance),
            "structured": StructuredFormatter(settings_instance),
        }

        if config:
            logging.config.dictConfig(config)
        else:
            # Настраиваем только уровень логирования для root логгера
            root_logger = logging.getLogger()
            root_logger.setLevel(getattr(logging, settings_instance.LOG_LEVEL.upper()))

    def get_logger(self, name: str, format: Optional[str] = None) -> logging.Logger:
        """Get logger instance.

        Args:
            name: Logger name, typically __name__
            format: Optional format override (json, text, structured)

        Returns:
            Logger instance
        """
        logger = logging.getLogger(name)

        # Добавляем handler только если у логгера нет хендлеров
        if len(logger.handlers) == 0:
            handler = logging.StreamHandler()
            # Используем указанный формат или дефолтный
            formatter = (
                self.formatters[format]
                if format
                else self.formatters[self.settings.LOG_FORMAT]
            )
            handler.setFormatter(formatter)
            logger.addHandler(handler)

        return logger
