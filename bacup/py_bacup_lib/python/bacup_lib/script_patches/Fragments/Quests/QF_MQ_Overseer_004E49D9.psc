Function Fragment_Stage_0005_Item_00()
    Actor playerRef = Alias_CurrentPlayer.GetReference() as Actor
    If playerRef != None && MQ_OverseerStarted != None
        playerRef.SetValue(MQ_OverseerStarted, 1.0)
    EndIf
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0010_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape01PickedUp)
EndFunction

Function Fragment_Stage_0015_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape01Played)
EndFunction

Function Fragment_Stage_0016_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape01APickedUp)
EndFunction

Function Fragment_Stage_0017_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape01APlayed)
EndFunction

Function Fragment_Stage_0022_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape02PickedUp)
EndFunction

Function Fragment_Stage_0025_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape02Played)
EndFunction

Function Fragment_Stage_0032_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape03PickedUp)
EndFunction

Function Fragment_Stage_0035_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape03Played)
EndFunction

Function Fragment_Stage_0042_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape04PickedUp)
EndFunction

Function Fragment_Stage_0045_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape04Played)
EndFunction

Function Fragment_Stage_0052_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape05PickedUp)
EndFunction

Function Fragment_Stage_0055_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape05Played)
EndFunction

Function Fragment_Stage_0062_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape06PickedUp)
EndFunction

Function Fragment_Stage_0065_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape06Played)
EndFunction

Function Fragment_Stage_0072_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape07PickedUp)
EndFunction

Function Fragment_Stage_0075_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape07Played)
EndFunction

Function Fragment_Stage_0082_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape08PickedUp)
EndFunction

Function Fragment_Stage_0085_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape08Played)
EndFunction

Function Fragment_Stage_0092_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape09PickedUp)
EndFunction

Function Fragment_Stage_0095_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape09Played)
EndFunction

Function Fragment_Stage_0102_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape10PickedUp)
EndFunction

Function Fragment_Stage_0105_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape10Played)
EndFunction

Function Fragment_Stage_0112_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape11PickedUp)
EndFunction

Function Fragment_Stage_0115_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape11Played)
EndFunction

Function Fragment_Stage_0122_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotape12PickedUp)
EndFunction

Function Fragment_Stage_0125_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotape12Played)
EndFunction

Function Fragment_Stage_0502_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotapeX1PickedUp)
EndFunction

Function Fragment_Stage_0505_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotapeX1Played)
EndFunction

Function Fragment_Stage_0512_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotapeX2PickedUp)
EndFunction

Function Fragment_Stage_0515_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotapeX2Played)
EndFunction

Function Fragment_Stage_0522_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotapeX3PickedUp)
EndFunction

Function Fragment_Stage_0525_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotapeX3Played)
EndFunction

Function Fragment_Stage_0532_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotapeX4PickedUp)
EndFunction

Function Fragment_Stage_0535_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotapeX4Played)
EndFunction

Function Fragment_Stage_0542_Item_00()
    RecordHolotapePickedUp(MQ_OverseerHolotapeX5PickedUp)
EndFunction

Function Fragment_Stage_0545_Item_00()
    RecordHolotapePlayed(MQ_OverseerHolotapeX5Played)
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_CurrentPlayer.GetReference() as Actor
    If playerRef != None && W05_MQ_Overseer_HasAllHolotapes != None
        playerRef.SetValue(W05_MQ_Overseer_HasAllHolotapes, 1.0)
    EndIf
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(1000)
EndFunction

Function RecordHolotapePickedUp(ActorValue akPickedUpValue)
    Actor playerRef = Alias_CurrentPlayer.GetReference() as Actor
    If playerRef == None || akPickedUpValue == None
        Return
    EndIf
    playerRef.SetValue(akPickedUpValue, 1.0)
    If !IsStageDone(1000) && HasEveryHolotape(playerRef)
        SetStage(1000)
    EndIf
EndFunction

Function RecordHolotapePlayed(ActorValue akPlayedValue)
    Actor playerRef = Alias_CurrentPlayer.GetReference() as Actor
    If playerRef != None && akPlayedValue != None
        playerRef.SetValue(akPlayedValue, 1.0)
    EndIf
EndFunction

Bool Function HasEveryHolotape(Actor akPlayer)
    If !HasHolotape(akPlayer, MQ_OverseerHolotape01PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape01APickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape02PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape03PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape04PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape05PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape06PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape07PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape08PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape09PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape10PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape11PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotape12PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotapeX1PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotapeX2PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotapeX3PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotapeX4PickedUp)
        Return False
    EndIf
    If !HasHolotape(akPlayer, MQ_OverseerHolotapeX5PickedUp)
        Return False
    EndIf
    Return True
EndFunction

Bool Function HasHolotape(Actor akPlayer, ActorValue akPickedUpValue)
    If akPickedUpValue == None
        Return False
    EndIf
    Return akPlayer.GetValue(akPickedUpValue) >= 1.0
EndFunction
