Event OnInit()
    GoToState("ready")
EndEvent

State ready
    Event OnActivate(ObjectReference akActionRef)
        If akActionRef != Game.GetPlayer() || MTR07_Earth == None || MTR07_Earth.IsCompleted() || MTR07_Earth.IsStageDone(QuestStageToSet)
            Return
        EndIf

        GoToState("busy")
        If !MTR07_Earth.IsRunning()
            If MTR07_EarthQuestStartKeyword != None
                MTR07_EarthQuestStartKeyword.SendStoryEventAndWait(akRef1 = akActionRef, akRef2 = Self)
            EndIf
            If !MTR07_Earth.IsRunning()
                GoToState("ready")
                Return
            EndIf
        EndIf

        If akActionRef.GetItemCount(IgnitionReactorCore01) < 1
            MTR07_EarthNoCoresMessage.Show()
            GoToState("ready")
            Return
        EndIf

        If MTR07_Earth.IsStageDone(QuestStageToSet)
            GoToState("ready")
            Return
        EndIf

        akActionRef.RemoveItem(IgnitionReactorCore01, 1, True)
        MTR07_Earth.SetStage(QuestStageToSet)
        If MTR07RRISoundRef != None
            MTR07RRISoundRef.PlayAnimation("Play")
        EndIf
        Disable()
    EndEvent
EndState
