from django.shortcuts import get_object_or_404, redirect, render
from django.contrib.auth.decorators import login_required
import yfinance as yf
import json
from django.contrib import messages
import pandas as pd
from ..models import DaftarEmiten
from django.core.paginator import Paginator
from django.http import JsonResponse
import logging
from django.db.models import Q

logger = logging.getLogger(__name__)


@login_required(login_url="/accounts/login/")
def daftar_saham(request):

    emiten = DaftarEmiten.objects.all()

    context = {
        "page_title": "DAFTAR SAHAM",
        "emiten" : emiten
        
        }

    return render(request, "tabel/daftar_emiten.html", context)


@login_required(login_url="/accounts/login/")
def daftar_saham_api(request):
    try:
        page = request.GET.get("page", 1)
        size = request.GET.get("size", 15)
        search = request.GET.get("search", "").strip()

        try:
            page = int(page)
            size = int(size)
        except (ValueError, TypeError):
            return JsonResponse(
                {"error": "Parameter page dan size harus angka positif"}, status=400
            )

        if page < 1 or size < 1:
            return JsonResponse(
                {"error": "Page dan size harus lebih besar dari 0"}, status=400
            )

        size = min(size, 100)

        base_queryset = DaftarEmiten.objects.only(
            "id", "kode_emiten", "nama_perusahaan"
        ).order_by("id")

        if search:
            base_queryset = base_queryset.filter(kode_emiten__istartswith=search)

        paginator = Paginator(base_queryset, size)
        page_obj = paginator.get_page(page)

        data = []
        for obj in page_obj.object_list:
            data.append(
                {
                    "id": obj.id,
                    "kode_emiten": obj.kode_emiten or "",
                    "nama_perusahaan": obj.nama_perusahaan or "",
                }
            )

        return JsonResponse(
            {
                "data": data,
                "last_page": paginator.num_pages,
                "total": paginator.count,
                "current_page": page,
                "per_page": size,
            }
        )

    except Exception as e:
        logger.error(f"Error fetching DaftarEmiten list: {e}", exc_info=True)
        return JsonResponse({"error": "Terjadi kesalahan server internal"}, status=500)


@login_required(login_url="/accounts/login/")
def delete_emiten(request, id_emiten):
    emiten = get_object_or_404(DaftarEmiten, pk=id_emiten)

    emiten.delete()
    messages.success(request, f"Emiten {emiten.kode_emiten} berhasil dihapus.")
    return redirect("daftar_saham:daftar_saham")


@login_required(login_url="/accounts/login/")
def delete_all_emiten(request):
    emiten = DaftarEmiten.objects.all()

    emiten.delete()
    messages.success(request, "Semua Emiten Berhasil dihapus")
    return redirect("daftar_saham:daftar_saham")
