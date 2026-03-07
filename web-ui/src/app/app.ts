import { Component, signal, computed } from '@angular/core';
import { RouterOutlet } from '@angular/router';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet],
  templateUrl: './app.html',
  styleUrl: './app.less'
})
export class App {
  protected readonly title = signal('Photometoria Web UI');
  protected readonly count = signal(0);
  protected readonly canClick = computed(() => this.count() < 10);

  clicked() {
    this.count.update((value) => value + 1);
  }
}
