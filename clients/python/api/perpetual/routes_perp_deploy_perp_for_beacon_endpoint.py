from http import HTTPStatus
from typing import Any, cast

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.api_response_for_deploy_perp_for_beacon_response import ApiResponseForDeployPerpForBeaconResponse
from ...models.deploy_perp_for_beacon_request import DeployPerpForBeaconRequest
from typing import cast



def _get_kwargs(
    *,
    body: DeployPerpForBeaconRequest,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}


    

    

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/deploy_perp_for_beacon",
    }

    _kwargs["json"] = body.to_dict()


    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Any | ApiResponseForDeployPerpForBeaconResponse:
    if response.status_code == 200:
        response_200 = ApiResponseForDeployPerpForBeaconResponse.from_dict(response.json())



        return response_200

    response_default = cast(Any, None)
    return response_default



def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[Any | ApiResponseForDeployPerpForBeaconResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient,
    body: DeployPerpForBeaconRequest,

) -> Response[Any | ApiResponseForDeployPerpForBeaconResponse]:
    """  Deploys a perpetual contract for a specific beacon.

    Creates a new perpetual pool using the PerpManager contract for the specified beacon address.
    Returns the perp ID, PerpManager address, and transaction hash on success.

    Args:
        body (DeployPerpForBeaconRequest): Deploy a perpetual contract for a specific beacon

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForDeployPerpForBeaconResponse]
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
    body: DeployPerpForBeaconRequest,

) -> Any | ApiResponseForDeployPerpForBeaconResponse | None:
    """  Deploys a perpetual contract for a specific beacon.

    Creates a new perpetual pool using the PerpManager contract for the specified beacon address.
    Returns the perp ID, PerpManager address, and transaction hash on success.

    Args:
        body (DeployPerpForBeaconRequest): Deploy a perpetual contract for a specific beacon

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForDeployPerpForBeaconResponse
     """


    return sync_detailed(
        client=client,
body=body,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient,
    body: DeployPerpForBeaconRequest,

) -> Response[Any | ApiResponseForDeployPerpForBeaconResponse]:
    """  Deploys a perpetual contract for a specific beacon.

    Creates a new perpetual pool using the PerpManager contract for the specified beacon address.
    Returns the perp ID, PerpManager address, and transaction hash on success.

    Args:
        body (DeployPerpForBeaconRequest): Deploy a perpetual contract for a specific beacon

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForDeployPerpForBeaconResponse]
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
    body: DeployPerpForBeaconRequest,

) -> Any | ApiResponseForDeployPerpForBeaconResponse | None:
    """  Deploys a perpetual contract for a specific beacon.

    Creates a new perpetual pool using the PerpManager contract for the specified beacon address.
    Returns the perp ID, PerpManager address, and transaction hash on success.

    Args:
        body (DeployPerpForBeaconRequest): Deploy a perpetual contract for a specific beacon

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForDeployPerpForBeaconResponse
     """


    return (await asyncio_detailed(
        client=client,
body=body,

    )).parsed
