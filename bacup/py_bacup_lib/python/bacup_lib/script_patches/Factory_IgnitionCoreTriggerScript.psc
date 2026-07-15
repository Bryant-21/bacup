Event OnInit()
    GoToState("ready")
EndEvent

State ready
    Event OnActivate(ObjectReference akActionRef)
        If akActionRef != Game.GetPlayer()
            Return
        EndIf

        If akActionRef.GetItemCount(IgnitionReactorCore01) < 1
            MTR07_EarthNoCoresMessage.Show()
            Return
        EndIf

        GoToState("busy")
        akActionRef.RemoveItem(IgnitionReactorCore01, 1, True)

        ObjectReference linkedRef = GetLinkedRef(LinkCustom01)
        If linkedRef != None
            linkedRef.Activate(akActionRef)
        EndIf

        If QuestStageToSet > 0
            UniqueFactoryQuest.SetStage(QuestStageToSet)
        EndIf
    EndEvent
EndState

State busy
    Event OnActivate(ObjectReference akActionRef)
    EndEvent
EndState
