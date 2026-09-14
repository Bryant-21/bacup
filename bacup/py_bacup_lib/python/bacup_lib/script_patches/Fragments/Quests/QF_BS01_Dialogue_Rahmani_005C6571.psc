Function Fragment_Stage_0200_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && BS01_PlayerHasMetRahmani_AV != None
        playerRef.SetValue(BS01_PlayerHasMetRahmani_AV, 1.0)
    EndIf
EndFunction
