from django.core.management.base import BaseCommand
from django_apscheduler.jobstores import DjangoJobStore
from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.interval import IntervalTrigger
from scanner_app.jobs import download_and_save_stocks
import logging

logger = logging.getLogger(__name__)

class Command(BaseCommand):
    help = "Jalankan scheduler: tiap 20 menit, hanya 09:00–15:00 (hari kerja)"

    def handle(self, *args, **options):
        scheduler = BlockingScheduler(timezone="Asia/Jakarta")
        scheduler.add_jobstore(DjangoJobStore(), "default")

        scheduler.add_job(
            download_and_save_stocks,
            trigger=IntervalTrigger(minutes=1),
            id="stock_download_20min_job",
            max_instances=1,
            replace_existing=True,
            misfire_grace_time=120,  # toleransi 2 menit
        )

        logger.info("🚀 Scheduler aktif!")
        logger.info("🔁 Akan cek tiap 20 menit, eksekusi hanya 09:00–15:00 (Senin–Jumat).")

        try:
            scheduler.start()
        except KeyboardInterrupt:
            logger.info("🛑 Scheduler dihentikan oleh pengguna.")
            scheduler.shutdown()