Function Fragment_End(Actor akActor)
    Quest host = Game.GetFormFromFile(0x005B79EB, "SeventySix.esm") as Quest
    If akActor == None || host == None || !host.IsStageDone(9000)
        Return
    EndIf
    Fragments:Quests:QF_BS01_Invention_005B79EB fragments = host as Fragments:Quests:QF_BS01_Invention_005B79EB
    If fragments != None && fragments.Alias_Valdez_Dungeon_Ref != None && akActor == fragments.Alias_Valdez_Dungeon_Ref.GetReference()
        akActor.DisableNoWait()
    EndIf
EndFunction
