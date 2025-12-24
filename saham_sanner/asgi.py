"""
ASGI config for saham_sanner project.

It exposes the ASGI callable as a module-level variable named ``application``.

For more information on this file, see
https://docs.djangoproject.com/en/5.2/howto/deployment/asgi/
"""

# import os

# from django.core.asgi import get_asgi_application

# os.environ.setdefault('DJANGO_SETTINGS_MODULE', 'saham_sanner.settings')

# application = get_asgi_application()



# saham_sanner/asgi.py
import os
from django.core.asgi import get_asgi_application
from channels.routing import ProtocolTypeRouter, URLRouter
# from channels.auth import AuthMiddlewareStack
from scanner_app.routing import websocket_urlpatterns  

os.environ.setdefault('DJANGO_SETTINGS_MODULE', 'saham_sanner.settings')


print("ASGI application is starting...")


application = ProtocolTypeRouter({
    "http": get_asgi_application(),
    "websocket": URLRouter(websocket_urlpatterns),
})