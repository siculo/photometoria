import { ChangeDetectionStrategy, Component, computed, inject, signal } from "@angular/core";
import { form, FormField, required } from "@angular/forms/signals";
import { APIService, InfoResponse } from "../service/APIService";

export interface NewTask {
    name: string;
    context: string;
}

@Component({
    selector: 'new-task-form',
    templateUrl: 'newTaskForm.html',
    styleUrl: 'newTaskForm.less',
    imports: [FormField],
    changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NewTaskForm {
    private api = inject(APIService);
    
    newTaskModel = signal({
        name: '',
        context: '',
    })

    newTaskForm = form(this.newTaskModel, (schemaPath) => {
        required(schemaPath.name, { message: 'Name is required' });
    });

    onSubmit(event: Event) {
        event.preventDefault();
        this.api.newTask(this.newTaskModel());
    }
}
