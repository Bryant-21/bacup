Function Fragment_Stage_0100_Item_00()
    TryStartBreadcrumb()
EndFunction

Function TryStartBreadcrumb()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef == None
        Stop()
        Return
    EndIf
    If playerRef.GetLevel() < 20
        StartTimer(30.0, 100)
        Return
    EndIf
    If BS01_MQ00_Breadcrumb_QuestStartKeyword != None
        BS01_MQ00_Breadcrumb_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    Stop()
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 100
        TryStartBreadcrumb()
    EndIf
EndEvent
