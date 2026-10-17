; GHL00 confrontation triggers (Asher at the ghoul camp, Madeline at Emmett): re-entering the trigger restarts the
; confrontation scene if it stopped before its TurnOffStage. Without a PrereqStage the scene must already have been
; started by the trigger's own DefaultAliasOnTriggerEnter stage.
Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || SceneToStart == None || SceneToStart.IsPlaying()
        Return
    EndIf
    Quest owner = GetOwningQuest()
    Int prereq = PrereqStage
    If prereq < 0
        DefaultAliasOnTriggerEnter stageTrigger = (Self as ReferenceAlias) as DefaultAliasOnTriggerEnter
        If stageTrigger != None
            prereq = stageTrigger.StageToSet
        EndIf
    EndIf
    If prereq >= 0 && !owner.IsStageDone(prereq)
        Return
    EndIf
    If TurnOffStage >= 0 && owner.IsStageDone(TurnOffStage)
        Return
    EndIf
    SceneToStart.Start()
EndEvent
