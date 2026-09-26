Function Fragment_End(Actor akActor)
    Quest host = Game.GetFormFromFile(0x005D2AFF, "SeventySix.esm") as Quest
    If akActor == None || host == None || !host.IsStageDone(700)
        Return
    EndIf
    akActor.DisableNoWait()
    If akActor.GetActorBase() == BS01_MQ05_Raiders_DungeonPierce && RaidersEnableMarker != None
        RaidersEnableMarker.DisableNoWait()
    EndIf
EndFunction
