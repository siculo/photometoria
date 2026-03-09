import { Component, signal, inject, effect, computed } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { NewTaskForm } from './task/newTaskForm';
import { Observable } from 'rxjs';
import { APIService, InfoResponse } from './service/APIService';
import { httpResource } from '@angular/common/http';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet],
  templateUrl: './app.html',
  styleUrl: './app.less'
})
export class App {
  protected readonly title = signal('Photometoria Web UI');
  info$!: Observable<InfoResponse>;
  private api = inject(APIService);

  protected readonly serverInfo = httpResource<InfoResponse>(() => '/api/info');

  protected readonly version = computed<string>(() => {
    if (this.serverInfo.hasValue()) {
      let v: string | null = (this.serverInfo.value())?.general?.version;
      return (v != null) ? v : "-";
    }
    return "-";
  });
}
