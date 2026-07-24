Function EvaluateIfPresent(ReferenceAlias akAlias)
    Actor target = akAlias.GetActorReference()
    If target != None
        target.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    EvaluateIfPresent(Alias_BandMember01)
    EvaluateIfPresent(Alias_BandMember02)
    EvaluateIfPresent(Alias_BandMember03)
    EvaluateIfPresent(Alias_BandMember04)
EndFunction

Function Fragment_Stage_0100_Item_00()
    EvaluateIfPresent(Alias_Audience01_Munch)
    EvaluateIfPresent(Alias_Audience02)
    EvaluateIfPresent(Alias_Audience03)
    EvaluateIfPresent(Alias_Audience04)
    EvaluateIfPresent(Alias_Audience05)
    EvaluateIfPresent(Alias_Audience06)
    EvaluateIfPresent(Alias_Audience07_Creed)
EndFunction

Function Fragment_Stage_1000_Item_00()
    EvaluateIfPresent(Alias_BandMember01)
    EvaluateIfPresent(Alias_BandMember02)
    EvaluateIfPresent(Alias_BandMember03)
    EvaluateIfPresent(Alias_BandMember04)
    EvaluateIfPresent(Alias_Audience01_Munch)
    EvaluateIfPresent(Alias_Audience02)
    EvaluateIfPresent(Alias_Audience03)
    EvaluateIfPresent(Alias_Audience04)
    EvaluateIfPresent(Alias_Audience05)
    EvaluateIfPresent(Alias_Audience06)
    EvaluateIfPresent(Alias_Audience07_Creed)
    EvaluateIfPresent(Alias_Guard01)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Stop()
EndFunction
