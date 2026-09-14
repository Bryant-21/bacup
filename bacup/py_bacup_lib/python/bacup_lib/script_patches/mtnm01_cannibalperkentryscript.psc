Event OnEntryRun(Int auiEntryID, ObjectReference akTarget, Actor akOwner)
    Actor targetActor = akTarget as Actor
    MTNM01QuestScript controller = MTNM01_Mayhem as MTNM01QuestScript
    If !controller || !controller.IsRunning() || !targetActor || !akOwner
        Return
    EndIf
    If akOwner != Game.GetPlayer() || !akOwner.HasKeyword(MTNM01_Mayhem_QuestActive_Keyword)
        Return
    EndIf
    If !controller.IsStageDone(controller.CannibalStage) || controller.IsStageDone(controller.CannibalSuccessStage)
        Return
    EndIf

    Race targetRace = targetActor.GetRace()
    If targetRace != FeralGhoulRace && targetRace != FeralGhoulGlowingRace
        Return
    EndIf
    If controller.GhoulCorpse
        controller.GhoulCorpse.ForceRefTo(targetActor)
    EndIf
    controller.SetStage(controller.CannibalSuccessStage)
EndEvent
