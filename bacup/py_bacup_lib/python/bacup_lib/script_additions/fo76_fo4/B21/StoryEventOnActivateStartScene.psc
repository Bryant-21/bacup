Scriptname B21:StoryEventOnActivateStartScene extends ObjectReference

Quest Property TargetQuest Auto Const
Keyword Property StoryEventKeyword Auto Const
Scene Property SceneToStart Auto Const

Auto State Armed
    Event OnActivate(ObjectReference akActionRef)
        Actor playerRef = Game.GetPlayer()
        If akActionRef != playerRef
            Return
        EndIf
        If TargetQuest == None || SceneToStart == None
            Return
        EndIf
        If TargetQuest.IsCompleted() || SceneToStart.IsPlaying()
            GoToState("Done")
            Return
        EndIf
        If StoryEventKeyword == None
            Return
        EndIf

        GoToState("Busy")
        If !TargetQuest.IsRunning()
            StoryEventKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
        If TargetQuest.IsRunning()
            If !SceneToStart.IsPlaying()
                SceneToStart.Start()
            EndIf
            If SceneToStart.IsPlaying()
                GoToState("Done")
                Return
            EndIf
        EndIf
        GoToState("Armed")
    EndEvent
EndState

State Busy
    Event OnActivate(ObjectReference akActionRef)
    EndEvent
EndState

State Done
    Event OnActivate(ObjectReference akActionRef)
    EndEvent
EndState
