Scriptname B21:QuestRewards Extends Quest
{Hands out the deterministic local reward rows this quest carried in FO76.

FO76 stores stage rewards in GMRW records hung off QUST.QRWD. The conversion
authors non-completion XP, caps, and explicit item rows onto this script.
Completion XP and notifications remain on FO4's native QUST.XNAM path.
B21:CurrencyQuestRewards restores scrip, bullion and Treasury Notes through
conditioned stage items. This script does not duplicate those currency rows.}

Int[] Property XPStages Auto Const
{Non-completion stages parallel to RewardXP.}

GlobalVariable[] Property RewardXP Auto
{XP amount globals parallel to XPStages.}

Int[] Property CapsStages Auto Const
{Stages carrying a locally resolvable caps row.}

GlobalVariable[] Property RewardCaps Auto
{Caps amount globals parallel to CapsStages.}

MiscObject Property CapsItem Auto
{FO4's bottlecap MISC.}

Int[] Property ItemStages Auto Const
{Stages parallel to RewardItems and RewardCounts.}

Form[] Property RewardItems Auto Const
{Explicit local reward payloads such as LVLI, MISC, ALCH, WEAP, or ARMO.}

Int[] Property RewardCounts Auto Const
{Parallel to RewardItems.}

Int[] Property XPConditionSets Auto Const
{Parallel to XPStages: the condition set gating the row, or -1 for none.}

Int[] Property CapsConditionSets Auto Const
{Parallel to CapsStages: the condition set gating the row, or -1 for none.}

Int[] Property ItemConditionSets Auto Const
{Parallel to ItemStages: the condition set gating the row, or -1 for none.
FO76 pays quest choices and event tiers from alternative reward groups.}

Int[] Property ConditionSetStarts Auto Const
{First Condition* row of each condition set; every row in a set must pass.}

Int[] Property ConditionSetCounts Auto Const
{Number of Condition* rows in each condition set.}

Int[] Property ConditionKinds Auto Const
{1 = the player's actor value, 2 = a global.}

Int[] Property ConditionOperators Auto Const
{CTDA comparison: 0 ==, 1 !=, 2 >, 3 >=, 4 <, 5 <=.}

Form[] Property ConditionForms Auto Const
{The ActorValue or GlobalVariable each condition reads.}

Float[] Property ConditionValues Auto Const
{The value each condition compares against.}

Int[] GrantedStages

Event OnQuestInit()
    GrantedStages = None
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    GrantRewardsForStage(auiStageID)
EndEvent

Function GrantRewardsForStage(Int auiStageID)
    If !HasRewardForStage(auiStageID) || HasGrantedStage(auiStageID)
        Return
    EndIf
    Quest owner = Self as Quest
    DefaultEventQuest eventQuest = owner as DefaultEventQuest
    If eventQuest != None && !eventQuest.IsPlayerParticipating()
        Return
    EndIf

    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    RecordGrantedStage(auiStageID)

    Int index = 0
    If XPStages != None && RewardXP != None
        While index < XPStages.Length && index < RewardXP.Length
            GlobalVariable xpAmount = RewardXP[index]
            If XPStages[index] == auiStageID && xpAmount != None && xpAmount.GetValueInt() > 0 && RowPasses(XPConditionSets, index, playerRef)
                Game.RewardPlayerXP(xpAmount.GetValueInt())
            EndIf
            index += 1
        EndWhile
    EndIf

    index = 0
    If CapsStages != None && RewardCaps != None && CapsItem != None
        While index < CapsStages.Length && index < RewardCaps.Length
            GlobalVariable capsAmount = RewardCaps[index]
            If CapsStages[index] == auiStageID && capsAmount != None && capsAmount.GetValueInt() > 0 && RowPasses(CapsConditionSets, index, playerRef)
                playerRef.AddItem(CapsItem, capsAmount.GetValueInt(), True)
            EndIf
            index += 1
        EndWhile
    EndIf

    index = 0
    If ItemStages != None && RewardItems != None && RewardCounts != None
        While index < ItemStages.Length && index < RewardItems.Length && index < RewardCounts.Length
            Form rewardItem = RewardItems[index]
            Int rewardCount = RewardCounts[index]
            If ItemStages[index] == auiStageID && rewardItem != None && rewardCount > 0 && RowPasses(ItemConditionSets, index, playerRef)
                playerRef.AddItem(rewardItem, rewardCount, True)
            EndIf
            index += 1
        EndWhile
    EndIf
EndFunction

Bool Function RowPasses(Int[] akSets, Int aiRow, Actor akPlayer)
    If akSets == None || aiRow >= akSets.Length || akSets[aiRow] < 0
        Return True
    EndIf
    Return ConditionSetPasses(akSets[aiRow], akPlayer)
EndFunction

Bool Function ConditionSetPasses(Int aiSet, Actor akPlayer)
    If ConditionSetStarts == None || ConditionSetCounts == None
        Return False
    EndIf
    If aiSet >= ConditionSetStarts.Length || aiSet >= ConditionSetCounts.Length
        Return False
    EndIf
    Int row = ConditionSetStarts[aiSet]
    Int lastRow = row + ConditionSetCounts[aiSet]
    While row < lastRow
        If !ConditionPasses(row, akPlayer)
            Return False
        EndIf
        row += 1
    EndWhile
    Return True
EndFunction

Bool Function ConditionPasses(Int aiRow, Actor akPlayer)
    If ConditionKinds == None || ConditionOperators == None || ConditionForms == None || ConditionValues == None
        Return False
    EndIf
    If aiRow >= ConditionKinds.Length || aiRow >= ConditionOperators.Length || aiRow >= ConditionForms.Length || aiRow >= ConditionValues.Length
        Return False
    EndIf
    Float current
    If ConditionKinds[aiRow] == 1
        ActorValue playerValue = ConditionForms[aiRow] as ActorValue
        If playerValue == None
            Return False
        EndIf
        current = akPlayer.GetValue(playerValue)
    ElseIf ConditionKinds[aiRow] == 2
        GlobalVariable globalValue = ConditionForms[aiRow] as GlobalVariable
        If globalValue == None
            Return False
        EndIf
        current = globalValue.GetValue()
    Else
        Return False
    EndIf
    Return Compare(ConditionOperators[aiRow], current, ConditionValues[aiRow])
EndFunction

Bool Function Compare(Int aiOperator, Float afCurrent, Float afExpected)
    If aiOperator == 0
        Return afCurrent == afExpected
    ElseIf aiOperator == 1
        Return afCurrent != afExpected
    ElseIf aiOperator == 2
        Return afCurrent > afExpected
    ElseIf aiOperator == 3
        Return afCurrent >= afExpected
    ElseIf aiOperator == 4
        Return afCurrent < afExpected
    ElseIf aiOperator == 5
        Return afCurrent <= afExpected
    EndIf
    Return False
EndFunction

Bool Function HasRewardForStage(Int auiStageID)
    Int index = 0
    If XPStages != None
        While index < XPStages.Length
            If XPStages[index] == auiStageID
                Return True
            EndIf
            index += 1
        EndWhile
    EndIf

    index = 0
    If CapsStages != None
        While index < CapsStages.Length
            If CapsStages[index] == auiStageID
                Return True
            EndIf
            index += 1
        EndWhile
    EndIf

    index = 0
    If ItemStages != None
        While index < ItemStages.Length
            If ItemStages[index] == auiStageID
                Return True
            EndIf
            index += 1
        EndWhile
    EndIf
    Return False
EndFunction

Bool Function HasGrantedStage(Int auiStageID)
    If GrantedStages == None
        Return False
    EndIf

    Int index = 0
    While index < GrantedStages.Length
        If GrantedStages[index] == auiStageID
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Function RecordGrantedStage(Int auiStageID)
    If GrantedStages == None
        GrantedStages = New Int[1]
        GrantedStages[0] = auiStageID
    Else
        GrantedStages.Add(auiStageID)
    EndIf
EndFunction
