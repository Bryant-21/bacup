Function Fragment_End(ObjectReference akSpeakerRef)
    Actor player = Game.GetPlayer()
    Float cooldownDays = W05_MQR_LouRepeatableRewardCooldown.GetValue() / 1440.0
    If Utility.GetCurrentGameTime() - player.GetValue(W05_MQR_LouRepeatableRewardTimeStampValue) >= cooldownDays
        player.AddItem(W05_LL_LouRepeatableReward, 1, False)
        player.SetValue(W05_MQR_LouRepeatableRewardTimeStampValue, Utility.GetCurrentGameTime())
    EndIf
EndFunction
