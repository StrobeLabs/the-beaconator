from http import HTTPStatus
from typing import Any, cast

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.api_response_for_batch_create_perpcity_beacon_response import ApiResponseForBatchCreatePerpcityBeaconResponse
from ...models.batch_create_perpcity_beacon_request import BatchCreatePerpcityBeaconRequest
from typing import cast



def _get_kwargs(
    *,
    body: BatchCreatePerpcityBeaconRequest,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}


    

    

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/batch_create_perpcity_beacon",
    }

    _kwargs["json"] = body.to_dict()


    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Any | ApiResponseForBatchCreatePerpcityBeaconResponse:
    if response.status_code == 200:
        response_200 = ApiResponseForBatchCreatePerpcityBeaconResponse.from_dict(response.json())



        return response_200

    response_default = cast(Any, None)
    return response_default



def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[Any | ApiResponseForBatchCreatePerpcityBeaconResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient,
    body: BatchCreatePerpcityBeaconRequest,

) -> Response[Any | ApiResponseForBatchCreatePerpcityBeaconResponse]:
    """  Creates multiple PerpCity beacons in a batch operation.

    Creates the specified number of beacons (1-100) via the beacon factory and registers them with the
    PerpCity registry. Returns details about successful and failed creations.

    Args:
        body (BatchCreatePerpcityBeaconRequest): Batch create multiple Perpcity beacons

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForBatchCreatePerpcityBeaconResponse]
     """


    kwargs = _get_kwargs(
        body=body,

    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)

def sync(
    *,
    client: AuthenticatedClient,
    body: BatchCreatePerpcityBeaconRequest,

) -> Any | ApiResponseForBatchCreatePerpcityBeaconResponse | None:
    """  Creates multiple PerpCity beacons in a batch operation.

    Creates the specified number of beacons (1-100) via the beacon factory and registers them with the
    PerpCity registry. Returns details about successful and failed creations.

    Args:
        body (BatchCreatePerpcityBeaconRequest): Batch create multiple Perpcity beacons

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForBatchCreatePerpcityBeaconResponse
     """


    return sync_detailed(
        client=client,
body=body,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient,
    body: BatchCreatePerpcityBeaconRequest,

) -> Response[Any | ApiResponseForBatchCreatePerpcityBeaconResponse]:
    """  Creates multiple PerpCity beacons in a batch operation.

    Creates the specified number of beacons (1-100) via the beacon factory and registers them with the
    PerpCity registry. Returns details about successful and failed creations.

    Args:
        body (BatchCreatePerpcityBeaconRequest): Batch create multiple Perpcity beacons

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForBatchCreatePerpcityBeaconResponse]
     """


    kwargs = _get_kwargs(
        body=body,

    )

    response = await client.get_async_httpx_client().request(
        **kwargs
    )

    return _build_response(client=client, response=response)

async def asyncio(
    *,
    client: AuthenticatedClient,
    body: BatchCreatePerpcityBeaconRequest,

) -> Any | ApiResponseForBatchCreatePerpcityBeaconResponse | None:
    """  Creates multiple PerpCity beacons in a batch operation.

    Creates the specified number of beacons (1-100) via the beacon factory and registers them with the
    PerpCity registry. Returns details about successful and failed creations.

    Args:
        body (BatchCreatePerpcityBeaconRequest): Batch create multiple Perpcity beacons

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForBatchCreatePerpcityBeaconResponse
     """


    return (await asyncio_detailed(
        client=client,
body=body,

    )).parsed
