import os
from celery import Celery

os.environ.setdefault('DJANGO_SETTINGS_MODULE', 'saham_sanner.settings')

app = Celery('saham_sanner')
app.config_from_object('django.conf:settings', namespace='CELERY')
app.autodiscover_tasks()