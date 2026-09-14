Scriptname B21:StoryEventOnTriggerEnter extends ObjectReference

Quest Property TargetQuest Auto Const
Keyword Property StoryEventKeyword Auto Const
Int Property StageToSet Auto Const

Auto State Armed
    Event OnTriggerEnter(ObjectReference akActionRef)
        Actor playerRef = Game.GetPlayer()
        If akActionRef != playerRef
            Return
        EndIf
        If TargetQuest == None || StoryEventKeyword == None
            Return
        EndIf
        If TargetQuest.IsCompleted()
            GoToState("Done")
            Return
        EndIf

        GoToState("Busy")
        If !TargetQuest.IsRunning()
            StoryEventKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
        If TargetQuest.IsRunning()
            TargetQuest.SetStage(StageToSet)
            GoToState("Done")
        Else
            GoToState("Armed")
        EndIf
    EndEvent
EndState

State Busy
    Event OnTriggerEnter(ObjectReference akActionRef)
    EndEvent
EndState

State Done
    Event OnTriggerEnter(ObjectReference akActionRef)
    EndEvent
EndState
