Scriptname B21:ExpeditionMissionRewards Extends Quest
{Grants the deterministic single-player completion tier for converted Expeditions.}

Int[] Property OptionalStages Auto Const
{The three optional objective success stages for this mission.}

GlobalVariable[] Property TierXP Auto
{XP amount globals for zero through three completed optional objectives.}

Int[] Property TierStampCounts Auto Const
{Stamp counts for zero through three completed optional objectives.}

MiscObject Property StampItem Auto
{The converted Stamp inventory item.}

Bool RewardGrantedThisRun
Bool CompletionReceiptReady
Int DeliveredXP
Int DeliveredStamps
Int[] DeliveredOptionalStates

Event OnQuestInit()
    ResetCompletionReceipt()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 0
        ResetCompletionReceipt()
    ElseIf auiStageID == 9000
        GrantCompletionReward()
    EndIf
EndEvent

Function GrantCompletionReward()
    If RewardGrantedThisRun || !RewardContractIsValid()
        Return
    EndIf

    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf

    Int tier = CompletedOptionalCount()
    GlobalVariable xpAmount = TierXP[tier]
    Int stampCount = TierStampCounts[tier]
    If xpAmount == None || xpAmount.GetValueInt() <= 0 || stampCount <= 0
        Return
    EndIf

    RewardGrantedThisRun = True
    DeliveredXP = xpAmount.GetValueInt()
    DeliveredStamps = stampCount
    DeliveredOptionalStates = CurrentOptionalStates()
    Game.RewardPlayerXP(xpAmount.GetValueInt())
    playerRef.AddItem(StampItem, stampCount, True)
    ; FO76's repeatable mission reward list (XPD_LL_Mission_Reward_Repeatable) pays two Legendary Modules.
    Form legendaryModule = Game.GetFormFromFile(0x005652F9, "SeventySix.esm")
    If legendaryModule
        playerRef.AddItem(legendaryModule, 2, True)
    EndIf
    CompletionReceiptReady = True
EndFunction

Bool Function HasDeliveredCompletionReward()
    Return RewardGrantedThisRun && CompletionReceiptReady
EndFunction

Int[] Function CompletionOptionalStates()
    If !HasDeliveredCompletionReward()
        Return New Int[0]
    EndIf
    Return DeliveredOptionalStates
EndFunction

Int Function CompletionXP()
    If !HasDeliveredCompletionReward()
        Return 0
    EndIf
    Return DeliveredXP
EndFunction

Int Function CompletionStamps()
    If !HasDeliveredCompletionReward()
        Return 0
    EndIf
    Return DeliveredStamps
EndFunction

Function ResetCompletionReceipt()
    RewardGrantedThisRun = False
    CompletionReceiptReady = False
    DeliveredXP = 0
    DeliveredStamps = 0
    DeliveredOptionalStates = None
EndFunction

Int[] Function CurrentOptionalStates()
    Int[] states = New Int[3]
    If !RewardContractIsValid()
        Return states
    EndIf

    Int index = 0
    While index < OptionalStages.Length
        states[index] = IsStageDone(OptionalStages[index]) as Int
        index += 1
    EndWhile
    Return states
EndFunction

Int Function CompletedOptionalCount()
    Int completed = 0
    Int index = 0
    While index < OptionalStages.Length
        If IsStageDone(OptionalStages[index])
            completed += 1
        EndIf
        index += 1
    EndWhile
    Return completed
EndFunction

Bool Function RewardContractIsValid()
    Return OptionalStages != None && OptionalStages.Length == 3 && TierXP != None && TierXP.Length == 4 && TierStampCounts != None && TierStampCounts.Length == 4 && StampItem != None
EndFunction
