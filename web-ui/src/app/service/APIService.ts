import { inject, Injectable } from "@angular/core";
import { HttpClient } from "@angular/common/http";
import { Observable } from "rxjs";

export interface General {
    version: string;
}
export interface Limits {
    max_concurrent_jobs: null | number;
    max_photo_size_bytes: null | number;
    max_photos_per_request: null | number;
}

export interface Server {
    active_tasks_count: number;
    allocated_space_bytes: number;
    available_providers: string[];
    available_space_bytes: number;
    default_provider: string;
    running_jobs_count: number;
    used_space_bytes: number;
}

export interface InfoResponse {
    general: General;
    limits: Limits;
    server: Server;
}

export interface NewTask {
    name: string;
    context: string;
}

@Injectable({providedIn: 'root'})
export class APIService {
    private http = inject(HttpClient);

    getInfo(): Observable<InfoResponse> {
        return this.http.get<InfoResponse>('/api/info', {
            mode: 'same-origin'
        });
    }

    newTask(t: NewTask): void {
        console.log('new task', t);
    }
}