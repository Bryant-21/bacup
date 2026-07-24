Event OnEntryRun(int auiEntryID, ObjectReference akTarget, Actor akOwner)
    If auiEntryID != PerkEntryID || !akTarget || !akOwner
        Return
    EndIf

    Actor targetActor = akTarget as Actor
    If !targetActor || targetActor == akOwner
        Return
    EndIf
    If !MTNM01_Mayhem.IsRunning() || !akOwner.HasKeyword(MTNM01_Mayhem_QuestActive_Keyword)
        Return
    EndIf
    If akOwner.GetValue(MTNM01_DeathclawFriendValue) >= 1.0
        Return
    EndIf

    akOwner.SetValue(MTNM01_DeathclawFriendValue, akOwner.GetValue(MTNM01_DeathclawFriendValue) + 1.0)
    If UIPerkAudio
        UIPerkAudio.Play(akOwner)
    EndIf
EndEvent
