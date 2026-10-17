Function Fragment_Stage_0150_Item_00()
    Actor playerRef = Game.GetPlayer()
    If BountyGiver_HasMet_AV != None && playerRef.GetValue(BountyGiver_HasMet_AV) < 1.0
        playerRef.SetValue(BountyGiver_HasMet_AV, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    ObjectReference marker = Alias_MapMarker_HWT.GetReference()
    If marker != None
        marker.AddToMap()
    EndIf
EndFunction

; The pointer quest's node only accepts a sender without Pointer_QuestActiveKeyword.
Function Fragment_Stage_0500_Item_00()
    Actor playerRef = Game.GetPlayer()
    If Pointer_QuestStartKeyword != None && (Pointer_QuestActiveKeyword == None || !playerRef.HasKeyword(Pointer_QuestActiveKeyword))
        Pointer_QuestStartKeyword.SendStoryEvent(None, playerRef)
    EndIf
EndFunction
