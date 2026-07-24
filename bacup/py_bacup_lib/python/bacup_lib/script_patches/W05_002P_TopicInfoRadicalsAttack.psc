Event OnEnd(ObjectReference akSpeakerRef, bool abHasBeenSaid)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_RadicalEnemyFaction != None
        playerRef.AddToFaction(W05_RadicalEnemyFaction)
    EndIf
    If playerRef != None && W05_RadicalFriendFaction != None
        playerRef.RemoveFromFaction(W05_RadicalFriendFaction)
    EndIf
    If playerRef != None && W05_MQ_002P_Radical_RadicalFriendValue != None
        playerRef.SetValue(W05_MQ_002P_Radical_RadicalFriendValue, 0.0)
    EndIf
EndEvent
