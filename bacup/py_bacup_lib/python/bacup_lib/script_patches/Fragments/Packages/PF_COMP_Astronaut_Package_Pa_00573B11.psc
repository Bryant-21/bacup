Function Fragment_End(Actor akActor)
    ; Assaultron_Instanced binds COMP_AstroSetStageOnPackageComplete (TargetPackage = this package,
    ; StageToSet = 3197); that alias script is hollow in the conversion, so complete the handoff here.
    Quest owningQuest = GetOwningQuest()
    If owningQuest && owningQuest.IsRunning() && !owningQuest.IsStageDone(3197)
        owningQuest.SetStage(3197)
    EndIf
EndFunction
