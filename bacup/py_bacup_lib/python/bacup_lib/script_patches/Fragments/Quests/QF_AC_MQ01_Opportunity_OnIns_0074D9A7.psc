Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AC_MQ01_Opportunity_StartKeyword != None
        AC_MQ01_Opportunity_StartKeyword.SendStoryEventAndWait(None, playerRef)
    EndIf
EndFunction
