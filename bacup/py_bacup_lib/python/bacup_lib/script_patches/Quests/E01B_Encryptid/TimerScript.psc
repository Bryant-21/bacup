; Assaultron recall cooldown (E01B_Encryptid_MasterQuest, cooldownTimeSeconds 10800): while onCooldown is 1 the recall
; terminal (TERM 56F554) shows its cooldown entry instead of "Use Keycard". The quest is RunOnce, so it stays running
; and each recall re-arms the timer instead of restarting the quest.
Function StartCooldown()
    onCooldown.SetValue(1.0)
    StartTimer(cooldownTimeSeconds)
EndFunction

Event OnTimer(Int aiTimerID)
    onCooldown.SetValue(0.0)
EndEvent

Event OnQuestShutdown()
    onCooldown.SetValue(0.0)
EndEvent
