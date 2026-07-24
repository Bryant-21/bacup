Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef == None || W05_MQR_JohnnyRepeatableRewardTimeStampValue == None || W05_LL_JohnnyRepeatableReward == None || W05_MQR_JohnnyRepeatablerewardCooldown == None
        Return
    EndIf
    Float currentTime = Utility.GetCurrentGameTime()
    Float lastGiven = akSpeakerRef.GetValue(W05_MQR_JohnnyRepeatableRewardTimeStampValue)
    Float cooldownDays = W05_MQR_JohnnyRepeatablerewardCooldown.GetValue() / 1440.0
    If lastGiven == 0.0 || currentTime - lastGiven >= cooldownDays
        Game.GetPlayer().AddItem(W05_LL_JohnnyRepeatableReward, 1)
        akSpeakerRef.SetValue(W05_MQR_JohnnyRepeatableRewardTimeStampValue, currentTime)
    EndIf
EndFunction
