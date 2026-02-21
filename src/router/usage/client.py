"""Usage API client."""
from datetime import datetime
from typing import Any, List, Optional, cast
from uuid import uuid4

import httpx
from pydantic import ValidationError

from core.logger import LoggerService
from core.settings import Settings
from providers.models import ProviderError
from .models import (
    CalculateCostRequest,
    CalculateCostResponse,
    Cost,
    CreateGenerationRequest,
    CreateGenerationResponse,
    CreateUsageRequest,
    CreateUsageResponse,
    Currency,
    FinalizeHoldRequest,
    FinalizeHoldResponse,
    FinalizeHoldWithTokensRequest,
    Generation,
    ModelRateResponse,
    ProcessCostRequest,
    ProcessCostResponse,
    ProcessCostWithTokensRequest,
    ServerCalculateCostRequest,
    ServerCreateGenerationRequest,
    ServerCreateUsageRequest,
    ServerFinalizeHoldRequest,
    ServerFinalizeHoldWithTokensRequest,
    ServerProcessCostRequest,
    ServerProcessCostWithTokensRequest,
    Usage,
)


class UsageClient:
    """Client for Usage API."""

    def __init__(self, settings: Settings, logger: LoggerService) -> None:
        """Initialize client.

        Args:
            settings: Application settings
            logger: Logger service
        """
        self.settings = settings
        self.base_url = settings.XSERVER_BASE_URL
        self.api_key = settings.XSERVER_API_KEY
        self.logger = logger.get_logger(__name__)
        self.client = self._create_client()

    def _create_client(self) -> httpx.AsyncClient:
        """Create and configure HTTP client.

        Returns:
            Configured HTTP client
        """
        return httpx.AsyncClient(
            base_url=self.base_url,
            headers={
                "Content-Type": "application/json",
            },
            timeout=300.0,  # 5 min timeout
        )

    def _get_headers(self, api_key: Optional[str] = None) -> dict[str, str]:
        """Get headers for request based on whether API key is present.

        Args:
            api_key: Optional API key for user authentication

        Returns:
            Headers dictionary
        """
        headers = {"Content-Type": "application/json"}

        if api_key:
            # For methods that require user auth (calculate_cost etc)
            headers["Authorization"] = f"Bearer {api_key}"
            headers["X-Service-Authorization"] = f"Bearer {self.api_key}"
        else:
            # For methods that only need service auth (model_rates)
            headers["Authorization"] = f"Bearer {self.api_key}"

        return headers

    async def _make_request(
        self,
        method: str,
        endpoint: str,
        json_data: Optional[dict[str, Any]] = None,
        params: Optional[dict[str, Any]] = None,
        api_key: Optional[str] = None,
    ) -> dict[str, Any]:
        """Make HTTP request to API.

        Args:
            method: HTTP method
            endpoint: API endpoint
            json_data: JSON data to send
            params: Query parameters to include in the URL

        Returns:
            Response data

        Raises:
            UsageAPIError: If request fails
        """
        # Prepare headers
        headers = self._get_headers(api_key)
        
        # Log request details
        self.logger.debug(
            f"USAGE API REQUEST: {method} {endpoint}",
            extra={
                "method": method,
                "endpoint": endpoint,
                "headers": {k: v for k, v in headers.items() if k != "Authorization" and k != "X-Service-Authorization"},
                "params": params,
                "json_data": json_data,
            },
        )
        
        try:
            # Make the request
            response = await self.client.request(
                method=method,
                url=endpoint,
                json=json_data,
                params=params,
                headers=headers,
            )
            
            # Get response data
            response_data = None
            try:
                response_data = response.json()
            except Exception as e:
                self.logger.warning(
                    f"Failed to parse response JSON: {str(e)}",
                    extra={"status_code": response.status_code, "text": response.text[:500]},
                )
                response_data = {"text": response.text[:500]}
            
            # Log response details
            self.logger.debug(
                f"USAGE API RESPONSE: {response.status_code} {method} {endpoint}",
                extra={
                    "method": method,
                    "endpoint": endpoint,
                    "status_code": response.status_code,
                    "response_data": response_data,
                },
            )
            
            # Raise for status to handle errors
            response.raise_for_status()
            return cast(dict[str, Any], response_data)
        except httpx.HTTPStatusError as e:
            error_msg = str(e)
            status_code = e.response.status_code
            response_data = None

            try:
                response_data = e.response.json()
                if isinstance(response_data, dict) and "error" in response_data:
                    error_msg = str(response_data["error"])
            except Exception:
                try:
                    response_data = {"text": e.response.text[:500]}
                except Exception:
                    response_data = {"text": "Unable to extract response text"}

            details = {
                "endpoint": endpoint,
                "method": method,
                "status_code": status_code,
            }
            if response_data:
                details["response_data"] = response_data
            if json_data:
                details["request_data"] = json_data

            self.logger.error(
                f"USAGE API ERROR: {status_code} {method} {endpoint} - {error_msg}",
                extra={
                    "method": method,
                    "endpoint": endpoint,
                    "status_code": status_code,
                    "error_message": error_msg,
                    "request_data": json_data,
                    "response_data": response_data,
                },
            )

            raise ProviderError(
                code=status_code,
                message=f"Usage API request failed: {error_msg}",
                details=details,
            )
        except httpx.RequestError as e:
            error_msg = str(e)
            details = {
                "endpoint": endpoint,
                "method": method,
                "status_code": 503,
                "network_error": True,
            }

            if json_data is not None:
                details["request_data"] = json_data

            self.logger.error(
                f"USAGE API NETWORK ERROR: 503 {method} {endpoint} - {error_msg}",
                extra={
                    "method": method,
                    "endpoint": endpoint,
                    "status_code": 503,
                    "error_message": error_msg,
                    "request_data": json_data,
                },
            )

            raise ProviderError(
                code=503,
                message=f"Usage API request failed: {error_msg}",
                details=details,
            )

    def _should_fallback(self, error: ProviderError) -> bool:
        """Check whether to degrade gracefully with synthetic zero responses."""
        if error.code in (500, 502, 503, 504):
            return True
        if isinstance(error.details, dict) and error.details.get("network_error"):
            return True
        return False

    def _fallback_transaction_id(self) -> str:
        return f"fallback_{uuid4().hex}"

    async def get_all_model_rates(
        self, currency: Optional[str] = None
    ) -> List[ModelRateResponse]:
        """Get all model rates.

        Args:
            currency: Optional currency to filter rates by (e.g. 'USD', 'RUB')

        Returns:
            List of model rates

        Raises:
            ProviderError: If request fails
        """
        self.logger.debug(
            "Getting all model rates",
            extra={"currency": currency} if currency else {},
        )

        try:
            params = {"currency": currency.upper()} if currency else None
            response_data = await self._make_request(
                "GET", "/models/rates", json_data=None, params=params
            )
            rates = [ModelRateResponse.model_validate(rate) for rate in response_data]

            self.logger.info(
                "Retrieved model rates",
                extra={"count": len(rates)},
            )

            return rates
        except ValidationError as e:
            raise ProviderError(
                code=400,
                message="Invalid model rates response format",
                details={"validation_errors": str(e)},
            )
        except ProviderError as e:
            if self._should_fallback(e):
                self.logger.warning(
                    "Usage API unavailable, returning empty model rates",
                    extra={"currency": currency, "error": e.message, "code": e.code},
                )
                return []
            new_error = ProviderError(
                code=e.code,
                message=f"Failed to get model rates: {e.message}",
                details=e.details,
            )
            raise new_error from e

    async def calculate_cost(
        self, request: CalculateCostRequest
    ) -> CalculateCostResponse:
        """Calculate cost for token usage.

        Args:
            request: Cost calculation request

        Returns:
            Cost calculation response

        Raises:
            ProviderError: If calculation fails
        """
        self.logger.debug(
            "Calculating cost for tokens",
            extra={
                "api_key": request.api_key,
                "model": request.token_count.model,
                "input_tokens": request.token_count.input,
                "output_tokens": request.token_count.output,
            },
        )

        try:
            # Convert to server request
            server_request = ServerCalculateCostRequest(
                token_count=request.token_count,
                currency=request.currency.upper() if request.currency else None,
            )
            response_data = await self._make_request(
                "POST",
                "/billing/costs/calculate",
                server_request.model_dump(),
                api_key=request.api_key,
            )
            response = cast(
                CalculateCostResponse,
                CalculateCostResponse.model_validate(response_data),
            )

            self.logger.info(
                "Calculated cost",
                extra={
                    "api_key": request.api_key,
                    "cost_amount": float(response.cost.amount),
                    "currency": response.cost.currency,
                },
            )

            return response
        except ValidationError as e:
            raise ProviderError(
                code=400,
                message="Invalid cost calculation response format",
                details={"validation_errors": str(e)},
            )
        except ProviderError as e:
            if self._should_fallback(e):
                self.logger.warning(
                    "Usage API unavailable, returning zero cost",
                    extra={
                        "api_key": request.api_key,
                        "model": request.token_count.model,
                        "error": e.message,
                        "code": e.code,
                    },
                )
                return CalculateCostResponse(
                    cost=Cost(
                        amount=0.0,
                        currency=request.currency or Currency.RUB,
                        breakdown={},
                    )
                )
            new_error = ProviderError(
                code=e.code,
                message=f"Cost calculation failed: {e.message}",
                details={"api_key": request.api_key, **e.details},
            )
            raise new_error from e

    async def process_cost(self, request: ProcessCostRequest) -> ProcessCostResponse:
        """Process cost for a user.

        Args:
            request: Cost processing request

        Returns:
            Cost processing response

        Raises:
            ProviderError: If processing fails
        """
        self.logger.debug(
            "Processing cost",
            extra={
                "api_key": request.api_key,
                "cost_amount": float(request.cost.amount),
                "currency": request.cost.currency,
            },
        )

        try:
            server_request = ServerProcessCostRequest(cost=request.cost)
            response_data = await self._make_request(
                "POST",
                "/billing/holds/create/cost",
                server_request.model_dump(),
                api_key=request.api_key,
            )
            response = cast(
                ProcessCostResponse, ProcessCostResponse.model_validate(response_data)
            )

            self.logger.info(
                "Processed cost",
                extra={
                    "api_key": request.api_key,
                    "amount_held": float(response.amount_held),
                },
            )

            return response
        except ValidationError as e:
            raise ProviderError(
                code=400,
                message="Invalid cost processing response format",
                details={"validation_errors": str(e)},
            )
        except ProviderError as e:
            # Специальная обработка для ошибки 402 Payment Required
            if e.code == 402:
                self.logger.warning(
                    "Payment required error during cost processing",
                    extra={
                        "api_key": request.api_key,
                        "cost_amount": float(request.cost.amount),
                        "currency": request.cost.currency,
                        "error_details": e.details,
                    },
                )
                new_error = ProviderError(
                    code=402,
                    message="Insufficient funds for request processing",
                    details={
                        "api_key": request.api_key,
                        "cost_amount": float(request.cost.amount),
                        "currency": request.cost.currency,
                        "error_type": "payment_required",
                    },
                )
            elif self._should_fallback(e):
                self.logger.warning(
                    "Usage API unavailable, returning zero hold for cost flow",
                    extra={
                        "api_key": request.api_key,
                        "error": e.message,
                        "code": e.code,
                    },
                )
                return ProcessCostResponse(
                    amount_held=0.0,
                    transaction_id=self._fallback_transaction_id(),
                )
            else:
                new_error = ProviderError(
                    code=e.code,
                    message=f"Cost processing failed: {e.message}",
                    details={"api_key": request.api_key, **e.details},
                )
            raise new_error from e

    async def finalize_hold(self, request: FinalizeHoldRequest) -> FinalizeHoldResponse:
        """Finalize cost hold for a user.

        Args:
            request: Hold finalization request

        Returns:
            Hold finalization response

        Raises:
            ProviderError: If finalization fails
        """
        self.logger.debug(
            "Finalizing cost hold",
            extra={
                "api_key": request.api_key,
                "cost_amount": float(request.cost.amount),
                "currency": request.cost.currency,
                "transaction_id": request.transaction_id,
            },
        )

        try:
            server_request = ServerFinalizeHoldRequest(
                cost=request.cost,
                transaction_id=request.transaction_id,
            )
            response_data = await self._make_request(
                "POST",
                "/billing/holds/finalize/cost",
                server_request.model_dump(),
                api_key=request.api_key,
            )
            response = cast(
                FinalizeHoldResponse, FinalizeHoldResponse.model_validate(response_data)
            )

            self.logger.info(
                "Finalized cost hold",
                extra={
                    "api_key": request.api_key,
                    "cost_amount": float(request.cost.amount),
                    "transaction_id": request.transaction_id,
                },
            )

            return response
        except ValidationError as e:
            raise ProviderError(
                code=400,
                message="Invalid finalize hold response format",
                details={"validation_errors": str(e)},
            )
        except ProviderError as e:
            if self._should_fallback(e):
                self.logger.warning(
                    "Usage API unavailable, returning successful finalize hold",
                    extra={
                        "api_key": request.api_key,
                        "transaction_id": request.transaction_id,
                        "error": e.message,
                        "code": e.code,
                    },
                )
                return FinalizeHoldResponse(success=True)
            new_error = ProviderError(
                code=e.code,
                message=f"Hold finalization failed: {e.message}",
                details={"api_key": request.api_key, **e.details},
            )
            raise new_error from e

    async def process_cost_with_tokens(
        self, request: ProcessCostWithTokensRequest
    ) -> ProcessCostResponse:
        """Process cost for a user using token count.

        Args:
            request: Cost processing request with token count

        Returns:
            Cost processing response

        Raises:
            ProviderError: If processing fails
        """
        self.logger.debug(
            "Processing cost with tokens",
            extra={
                "api_key": request.api_key,
                "model": request.token_count.model,
                "input_tokens": request.token_count.input,
                "output_tokens": request.token_count.output,
            },
        )

        try:
            server_request = ServerProcessCostWithTokensRequest(
                token_count=request.token_count
            )
            response_data = await self._make_request(
                "POST",
                "/billing/holds/create/tokens",
                server_request.model_dump(),
                api_key=request.api_key,
            )
            response = cast(
                ProcessCostResponse, ProcessCostResponse.model_validate(response_data)
            )

            self.logger.info(
                "Processed cost with tokens",
                extra={
                    "api_key": request.api_key,
                    "amount_held": float(response.amount_held),
                },
            )

            return response
        except ValidationError as e:
            raise ProviderError(
                code=400,
                message="Invalid cost processing response format",
                details={"validation_errors": str(e)},
            )
        except ProviderError as e:
            # Специальная обработка для ошибки 402 Payment Required
            if e.code == 402:
                self.logger.warning(
                    "Payment required error during cost processing with tokens",
                    extra={
                        "api_key": request.api_key,
                        "model": request.token_count.model,
                        "input_tokens": request.token_count.input,
                        "output_tokens": request.token_count.output,
                        "error_details": e.details,
                    },
                )
                new_error = ProviderError(
                    code=402,
                    message="Insufficient funds for request processing",
                    details={
                        "api_key": request.api_key,
                        "model": request.token_count.model,
                        "error_type": "payment_required",
                    },
                )
            elif self._should_fallback(e):
                self.logger.warning(
                    "Usage API unavailable, returning zero hold for token flow",
                    extra={
                        "api_key": request.api_key,
                        "model": request.token_count.model,
                        "error": e.message,
                        "code": e.code,
                    },
                )
                return ProcessCostResponse(
                    amount_held=0.0,
                    transaction_id=self._fallback_transaction_id(),
                )
            else:
                new_error = ProviderError(
                    code=e.code,
                    message=f"Cost processing with tokens failed: {e.message}",
                    details={"api_key": request.api_key, **e.details},
                )
            raise new_error from e

    async def finalize_hold_with_tokens(
        self, request: FinalizeHoldWithTokensRequest
    ) -> FinalizeHoldResponse:
        """Finalize cost hold for a user using token count.

        Args:
            request: Hold finalization request with token count

        Returns:
            Hold finalization response

        Raises:
            ProviderError: If finalization fails
        """
        self.logger.debug(
            "Finalizing cost hold with tokens",
            extra={
                "api_key": request.api_key,
                "model": request.token_count.model,
                "input_tokens": request.token_count.input,
                "output_tokens": request.token_count.output,
                "transaction_id": request.transaction_id,
            },
        )

        try:
            server_request = ServerFinalizeHoldWithTokensRequest(
                token_count=request.token_count,
                transaction_id=request.transaction_id,
            )
            response_data = await self._make_request(
                "POST",
                "/billing/holds/finalize/tokens",
                server_request.model_dump(),
                api_key=request.api_key,
            )
            response = cast(
                FinalizeHoldResponse, FinalizeHoldResponse.model_validate(response_data)
            )

            self.logger.info(
                "Finalized cost hold with tokens",
                extra={
                    "api_key": request.api_key,
                    "model": request.token_count.model,
                    "transaction_id": request.transaction_id,
                },
            )

            return response
        except ValidationError as e:
            raise ProviderError(
                code=400,
                message="Invalid finalize hold with tokens response format",
                details={"validation_errors": str(e)},
            )
        except ProviderError as e:
            if self._should_fallback(e):
                self.logger.warning(
                    "Usage API unavailable, returning successful finalize hold with tokens",
                    extra={
                        "api_key": request.api_key,
                        "transaction_id": request.transaction_id,
                        "model": request.token_count.model,
                        "error": e.message,
                        "code": e.code,
                    },
                )
                return FinalizeHoldResponse(success=True)
            new_error = ProviderError(
                code=e.code,
                message=f"Hold finalization with tokens failed: {e.message}",
                details={"api_key": request.api_key, **e.details},
            )
            raise new_error from e

    async def create_usage(self, request: CreateUsageRequest) -> CreateUsageResponse:
        """Create usage record.

        Args:
            request: Usage record creation request

        Returns:
            Usage creation response

        Raises:
            ProviderError: If creation fails
        """
        self.logger.debug(
            "Creating usage record",
            extra={
                "api_key": request.api_key,
                "model": request.tokens.model,
                "cost": float(request.cost.amount),
            },
        )

        try:
            # Convert to server request
            server_request = ServerCreateUsageRequest(
                tokens=request.tokens,
                cost=request.cost,
                meta_info=request.meta_info,
            )
            response_data = await self._make_request(
                "POST",
                "/analytics/usage",
                server_request.model_dump(),
                api_key=request.api_key,
            )
            response = cast(
                CreateUsageResponse, CreateUsageResponse.model_validate(response_data)
            )

            self.logger.info(
                "Created usage record",
                extra={
                    "api_key": request.api_key,
                    "usage_id": str(response.data.id),
                },
            )

            return response
        except ValidationError as e:
            raise ProviderError(
                code=400,
                message="Invalid usage record creation response format",
                details={"validation_errors": str(e)},
            )
        except ProviderError as e:
            if self._should_fallback(e):
                self.logger.warning(
                    "Usage API unavailable, returning synthetic usage record",
                    extra={
                        "api_key": request.api_key,
                        "model": request.tokens.model,
                        "error": e.message,
                        "code": e.code,
                    },
                )
                return CreateUsageResponse(
                    data=Usage(
                        id=uuid4(),
                        model=request.tokens.model,
                        prompt_tokens=request.tokens.input,
                        completion_tokens=request.tokens.output,
                        total_cost=float(request.cost.amount),
                        currency=request.cost.currency,
                        meta_info=request.meta_info,
                        created_at=datetime.utcnow(),
                    )
                )
            new_error = ProviderError(
                code=e.code,
                message=f"Usage record creation failed: {e.message}",
                details={"api_key": request.api_key, **e.details},
            )
            raise new_error from e

    async def create_generation(
        self, request: CreateGenerationRequest
    ) -> CreateGenerationResponse:
        """Create generation record.

        Args:
            request: Generation record creation request

        Returns:
            Generation creation response

        Raises:
            ProviderError: If creation fails
        """
        self.logger.debug(
            "Creating generation record",
            extra={
                "api_key": request.api_key,
                "usage_id": str(request.usage_id),
                "generation_id": str(request.id),
                "model": request.model,
            },
        )

        try:
            # Convert to server request
            server_request = ServerCreateGenerationRequest(
                id=request.id,
                model=request.model,
                provider=request.provider,
                origin=request.origin,
                generation_time=request.generation_time,
                speed=request.speed,
                finish_reason=request.finish_reason,
                native_finish_reason=request.native_finish_reason,
                error=request.error,
                is_streaming=request.is_streaming,
                meta_info=request.meta_info,
                usage_id=request.usage_id,
            )
            response_data = await self._make_request(
                "POST",
                "/analytics/generation",
                server_request.model_dump(),
                api_key=request.api_key,
            )
            response = cast(
                CreateGenerationResponse,
                CreateGenerationResponse.model_validate(response_data),
            )

            self.logger.info(
                "Created generation record",
                extra={
                    "api_key": request.api_key,
                    "usage_id": str(request.usage_id),
                    "generation_id": request.id,
                },
            )

            return response
        except ValidationError as e:
            raise ProviderError(
                code=400,
                message="Invalid generation record creation response format",
                details={"validation_errors": str(e)},
            )
        except ProviderError as e:
            if self._should_fallback(e):
                self.logger.warning(
                    "Usage API unavailable, returning synthetic generation record",
                    extra={
                        "api_key": request.api_key,
                        "generation_id": request.id,
                        "model": request.model,
                        "error": e.message,
                        "code": e.code,
                    },
                )
                return CreateGenerationResponse(
                    data=Generation(
                        id=request.id,
                        total_cost=0.0,
                        created_at=datetime.utcnow().isoformat(),
                        model=request.model,
                        origin=request.origin or "",
                        usage=0.0,
                        is_byok=False,
                        streamed=request.is_streaming,
                        finish_reason=request.finish_reason,
                        native_finish_reason=request.native_finish_reason,
                    )
                )
            new_error = ProviderError(
                code=e.code,
                message=f"Generation record creation failed: {e.message}",
                details={"api_key": request.api_key, **e.details},
            )
            raise new_error from e

    async def __aenter__(self) -> "UsageClient":
        """Enter async context manager."""
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Exit async context manager."""
        await self.client.aclose()
